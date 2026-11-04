use super::*;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::ports::{PortError, WaterRepository};
use crate::domain::water::{WaterDrink, WaterPlan};
use crate::infrastructure::water_store::JsonWaterRepository;
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Local, TimeZone};

#[derive(Default)]
struct MemoryWaterRepository {
    drinks: Mutex<Vec<WaterDrink>>,
    fail_save: AtomicBool,
    saves: AtomicUsize,
}

impl MemoryWaterRepository {
    fn with_drinks(drinks: Vec<WaterDrink>) -> Self {
        Self {
            drinks: Mutex::new(drinks),
            ..Self::default()
        }
    }
}

impl WaterRepository for MemoryWaterRepository {
    fn load(&self) -> Result<Vec<WaterDrink>, PortError> {
        Ok(self.drinks.lock().unwrap().clone())
    }

    fn save(&self, drinks: &[WaterDrink]) -> Result<(), PortError> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(PortError::new("water.error.write", "mock save failure"));
        }
        *self.drinks.lock().unwrap() = drinks.to_vec();
        Ok(())
    }
}

fn local_at(hour: u32, minute: u32, second: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 11, 2, hour, minute, second)
        .single()
        .unwrap()
}

fn enabled_plan(goal_ml: u16, bottle_ml: u16) -> WaterPlan {
    WaterPlan {
        enabled: true,
        goal_ml,
        bottle_ml,
        ..WaterPlan::default()
    }
}

fn service(
    drinks: Vec<WaterDrink>,
    plan: WaterPlan,
    now: DateTime<Local>,
) -> (WaterService, Arc<MemoryWaterRepository>) {
    let repository = Arc::new(MemoryWaterRepository::with_drinks(drinks));
    let repository_port: Arc<dyn WaterRepository> = repository.clone();
    let service = WaterService::load(repository_port, plan, now).unwrap();
    (service, repository)
}

fn drink_at(at: DateTime<Local>, amount_ml: u16) -> WaterDrink {
    WaterDrink {
        at: at.fixed_offset(),
        amount_ml,
    }
}

#[test]
fn default_plan_is_valid_and_uses_500ml_bottle_1500ml_goal_and_workday() {
    let plan = WaterPlan::default();

    assert!(!plan.enabled);
    assert_eq!(plan.goal_ml, 1500);
    assert_eq!(plan.bottle_ml, 500);
    assert_eq!(plan.active_from_minute, 8 * 60);
    assert_eq!(plan.active_until_minute, 17 * 60);
    assert!(plan.is_valid());
}

#[test]
fn plans_reject_zero_values_and_invalid_active_windows() {
    let plan = WaterPlan {
        goal_ml: 0,
        ..WaterPlan::default()
    };
    assert!(!plan.is_valid());

    let plan = WaterPlan {
        bottle_ml: 0,
        ..WaterPlan::default()
    };
    assert!(!plan.is_valid());

    let mut plan = WaterPlan::default();
    plan.active_from_minute = plan.active_until_minute;
    assert!(!plan.is_valid());

    let plan = WaterPlan {
        active_until_minute: 1441,
        ..WaterPlan::default()
    };
    assert!(!plan.is_valid());
}

#[test]
fn all_four_water_goal_choices_are_valid_plan_values() {
    for goal_ml in [1500, 2000, 2500, 3000] {
        let plan = enabled_plan(goal_ml, 500);
        assert!(plan.is_valid(), "{goal_ml} ml should be accepted");
    }
}

#[test]
fn default_plan_spreads_goal_into_doses_and_bottle_sips() {
    let plan = enabled_plan(1500, 500);

    assert_eq!(plan.reminder_count(), 36);
    assert_eq!(plan.dose_ml(), 42);
    assert_eq!(plan.sips_per_bottle(), 12);
    assert_eq!(plan.refills(), 2);
}

#[test]
fn reminders_are_spaced_every_fifteen_minutes_inside_the_half_open_active_window() {
    let plan = enabled_plan(2500, 500);
    assert_eq!(plan.reminder_count(), 36);
    assert_eq!(plan.dose_ml(), 70);
    assert_eq!(plan.sips_per_bottle(), 8);
    assert_eq!(plan.refills(), 4);

    let mut cursor = 0;
    let mut slots = Vec::new();
    for index in 0..36 {
        let (day_offset, second) = plan.next_slot(cursor).unwrap();
        assert_eq!(day_offset, 0);
        assert_eq!(second, 8 * 3600 + index * 15 * 60);
        slots.push(second);
        cursor = second;
    }
    assert_eq!(slots.len(), 36);
    assert_eq!(plan.next_slot(8 * 3600), Some((0, 8 * 3600 + 15 * 60)));
    assert_eq!(plan.next_slot(16 * 3600 + 45 * 60), Some((1, 8 * 3600)));
    assert_eq!(plan.next_slot(17 * 3600), Some((1, 8 * 3600)));
}

#[test]
fn disabled_plan_has_no_schedule_and_enabled_default_starts_at_0800() {
    assert_eq!(WaterPlan::default().next_slot(0), None);
    let plan = enabled_plan(1500, 500);
    assert_eq!(plan.next_slot(7 * 3600 + 59 * 60), Some((0, 8 * 3600)));
    assert_eq!(plan.next_slot(8 * 3600), Some((0, 8 * 3600 + 15 * 60)));
    assert_eq!(plan.next_slot(14 * 3600), Some((0, 14 * 3600 + 15 * 60)));
}

#[test]
fn daily_totals_group_fixed_offset_drinks_by_local_date_and_hour() {
    let now = local_at(12, 30, 0);
    let drinks = [
        WaterDrink {
            at: local_at(12, 5, 0).with_timezone(&FixedOffset::east_opt(0).unwrap()),
            amount_ml: 500,
        },
        drink_at(local_at(9, 20, 0), 250),
        drink_at(local_at(10, 0, 0) - ChronoDuration::days(1), 750),
        drink_at(local_at(13, 0, 0) + ChronoDuration::days(1), 1000),
    ];

    let (total, hourly) = daily_totals(&drinks, now);

    assert_eq!(total, 750);
    assert_eq!(hourly[9], 250);
    assert_eq!(hourly[12], 500);
    assert_eq!(hourly.iter().sum::<u64>(), total);
}

#[test]
fn service_schedules_first_due_at_0800_and_then_evenly_spaced_slots() {
    let (mut service, _) = service(Vec::new(), enabled_plan(1500, 500), local_at(7, 59, 0));
    let first_due = local_at(8, 0, 0);

    assert_eq!(service.state(local_at(7, 59, 0)).next_at, Some(first_due));
    assert!(!service.tick(local_at(7, 59, 59)));
    assert!(service.tick(first_due));
    assert!(!service.tick(first_due));
    let next_due = local_at(8, 15, 0);
    assert_eq!(service.state(first_due).next_at, Some(next_due));
    assert!(!service.tick(local_at(8, 14, 59)));
    assert!(service.tick(next_due));
}

#[test]
fn reminder_is_not_due_at_or_after_1700_and_resumes_next_day_at_0800() {
    let (mut service, _) = service(Vec::new(), enabled_plan(1500, 500), local_at(16, 59, 0));

    assert!(!service.tick(local_at(17, 0, 0)));
    assert_eq!(
        service.state(local_at(17, 0, 0)).next_at,
        Some(local_at(8, 0, 0) + ChronoDuration::days(1))
    );
    assert!(!service.tick(local_at(0, 0, 0) + ChronoDuration::days(1)));
    assert!(service.tick(local_at(8, 0, 0) + ChronoDuration::days(1)));
}

#[test]
fn reaching_goal_disables_remaining_slots_until_next_day() {
    let now = local_at(7, 59, 0);
    let (mut service, _) = service(Vec::new(), enabled_plan(500, 500), now);
    for index in 0..36 {
        let due = local_at(8, 0, 0) + ChronoDuration::minutes(i64::from(index * 15));
        assert!(service.tick(due));
        service.drink(due).unwrap();
    }
    assert_eq!(service.state(local_at(16, 45, 0)).consumed_ml, 504);
    assert!(!service.tick(local_at(17, 0, 0)));
    assert_eq!(
        service.state(local_at(17, 0, 0)).next_at,
        Some(local_at(8, 0, 0) + ChronoDuration::days(1))
    );
}

#[test]
fn drinks_persist_the_scheduled_dose_and_reload_as_today_history() {
    let now = local_at(9, 15, 0);
    let earlier = drink_at(local_at(8, 0, 0), 500);
    let (mut service, repository) = service(vec![earlier.clone()], enabled_plan(2000, 750), now);

    assert_eq!(service.state(now).consumed_ml, 500);
    assert_eq!(service.state(now).hourly_ml[8], 500);
    assert!(service.drink(now).unwrap());
    assert_eq!(service.state(now).consumed_ml, 556);
    assert_eq!(service.state(now).hourly_ml[9], 56);
    assert_eq!(repository.saves.load(Ordering::SeqCst), 1);

    let repository_port: Arc<dyn WaterRepository> = repository.clone();
    let reloaded = WaterService::load(repository_port, enabled_plan(2000, 750), now).unwrap();
    assert_eq!(reloaded.state(now), service.state(now));
    let persisted = repository.drinks.lock().unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0], earlier);
    assert_eq!(persisted[1].amount_ml, 56);
}

#[test]
fn disabled_plan_records_manual_drinks_without_scheduling_reminders() {
    let now = local_at(8, 0, 0);
    let (mut service, repository) = service(Vec::new(), WaterPlan::default(), now);

    assert!(service.drink(now).unwrap());
    assert!(!service.tick(now + ChronoDuration::days(1)));
    assert_eq!(service.state(now).next_at, None);
    assert_eq!(service.state(now).consumed_ml, 42);
    assert_eq!(repository.drinks.lock().unwrap().len(), 1);
    assert_eq!(repository.drinks.lock().unwrap()[0].amount_ml, 42);
    assert_eq!(repository.saves.load(Ordering::SeqCst), 1);
}

#[test]
fn reset_today_persists_only_removal_of_current_local_day_drinks() {
    let today = local_at(10, 0, 0);
    let yesterday = drink_at(local_at(16, 0, 0) - ChronoDuration::days(1), 250);
    let today_drink = drink_at(local_at(9, 0, 0), 500);
    let (mut service, repository) = service(
        vec![yesterday.clone(), today_drink],
        enabled_plan(2000, 500),
        today,
    );

    assert_eq!(service.state(today).consumed_ml, 500);
    assert!(service.reset_today(today).unwrap());
    assert_eq!(service.state(today).consumed_ml, 0);
    assert_eq!(*repository.drinks.lock().unwrap(), [yesterday]);
    assert!(!service.reset_today(today).unwrap());
    assert_eq!(repository.saves.load(Ordering::SeqCst), 1);
}

#[test]
fn repository_failure_does_not_change_water_state_or_history() {
    let now = local_at(10, 0, 0);
    let existing = drink_at(local_at(9, 0, 0), 250);
    let (mut service, repository) = service(vec![existing.clone()], enabled_plan(2000, 500), now);
    repository.fail_save.store(true, Ordering::SeqCst);

    assert!(service.drink(now).is_err());
    assert!(service.reset_today(now).is_err());
    assert_eq!(service.state(now).consumed_ml, 250);
    assert_eq!(*repository.drinks.lock().unwrap(), [existing]);
    assert_eq!(repository.saves.load(Ordering::SeqCst), 2);
}

#[test]
fn json_repository_round_trips_history_and_rejects_wrong_schema_or_zero_volume() {
    let path = std::env::temp_dir().join(format!("lumi-water-{}.json", uuid::Uuid::new_v4()));
    let repository = JsonWaterRepository::new(path.clone());
    let drinks = vec![drink_at(local_at(9, 30, 0), 500)];

    repository.save(&drinks).unwrap();
    assert_eq!(repository.load().unwrap(), drinks);

    std::fs::write(&path, r#"{"schema_version":2,"drinks":[]}"#).unwrap();
    assert!(repository.load().is_err());

    std::fs::write(&path, r#"{"schema_version":1,"drinks":[],"obsolete":true}"#).unwrap();
    assert!(repository.load().is_err());

    let zero_volume = serde_json::json!({
        "schema_version": 1,
        "drinks": [{ "at": local_at(9, 30, 0).fixed_offset(), "amount_ml": 0 }]
    });
    std::fs::write(&path, serde_json::to_vec(&zero_volume).unwrap()).unwrap();
    assert!(repository.load().is_err());

    let _ = std::fs::remove_file(path);
}

#[test]
fn water_history_keeps_each_drink_offset_when_serialized() {
    let fixed = FixedOffset::east_opt(5 * 3600).unwrap();
    let instant = fixed
        .with_ymd_and_hms(2026, 11, 2, 9, 45, 0)
        .single()
        .unwrap();
    let drink = WaterDrink {
        at: instant,
        amount_ml: 500,
    };
    let encoded = serde_json::to_string(&drink).unwrap();
    let decoded: WaterDrink = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, drink);
    assert_eq!(decoded.at.offset().local_minus_utc(), 5 * 3600);
}
