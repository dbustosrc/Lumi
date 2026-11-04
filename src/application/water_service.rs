use crate::domain::ports::{PortError, WaterRepository};
use crate::domain::water::{WaterDrink, WaterPlan, WaterState, daily_totals};
use chrono::{DateTime, Duration, Local, TimeZone, Timelike};
use std::sync::Arc;

pub struct WaterService {
    repository: Arc<dyn WaterRepository>,
    drinks: Vec<WaterDrink>,
    plan: WaterPlan,
    next_at: Option<DateTime<Local>>,
    day: chrono::NaiveDate,
}

impl WaterService {
    pub fn load(
        repository: Arc<dyn WaterRepository>,
        plan: WaterPlan,
        now: DateTime<Local>,
    ) -> Result<Self, PortError> {
        if !plan.is_valid() {
            return Err(PortError::new(
                "settings.status.invalidWater",
                "invalid water plan",
            ));
        }
        let drinks = repository.load()?;
        let mut service = Self {
            repository,
            drinks,
            plan,
            next_at: None,
            day: now.date_naive(),
        };
        service.reschedule(now);
        Ok(service)
    }

    pub fn configure(&mut self, plan: &WaterPlan, now: DateTime<Local>) {
        if self.plan != *plan {
            self.plan = plan.clone();
            self.reschedule(now);
        }
    }

    pub fn drink(&mut self, now: DateTime<Local>) -> Result<bool, PortError> {
        let mut next = self.drinks.clone();
        next.push(WaterDrink {
            at: now.fixed_offset(),
            amount_ml: self.plan.dose_ml(),
        });
        self.repository.save(&next)?;
        self.drinks = next;
        self.reschedule(now);
        Ok(true)
    }

    pub fn reset_today(&mut self, now: DateTime<Local>) -> Result<bool, PortError> {
        let next: Vec<_> = self
            .drinks
            .iter()
            .filter(|drink| drink.at.with_timezone(&Local).date_naive() != now.date_naive())
            .cloned()
            .collect();
        if next.len() == self.drinks.len() {
            return Ok(false);
        }
        self.repository.save(&next)?;
        self.drinks = next;
        self.reschedule(now);
        Ok(true)
    }

    pub fn tick(&mut self, now: DateTime<Local>) -> bool {
        let expired = self.next_at.is_some_and(|at| now >= at);
        if self.day != now.date_naive() {
            self.day = now.date_naive();
            if !expired {
                self.reschedule(now);
            }
        }
        let second = now.num_seconds_from_midnight();
        let due = expired
            && self.plan.enabled
            && second >= u32::from(self.plan.active_from_minute) * 60
            && second < u32::from(self.plan.active_until_minute) * 60
            && daily_totals(&self.drinks, now).0 < u64::from(self.plan.goal_ml);
        if expired {
            self.reschedule(now);
        }
        due
    }

    fn reschedule(&mut self, now: DateTime<Local>) {
        let (consumed, _) = daily_totals(&self.drinks, now);
        let slot = if consumed >= u64::from(self.plan.goal_ml) {
            self.plan
                .enabled
                .then_some((1, u32::from(self.plan.active_from_minute) * 60))
        } else {
            self.plan.next_slot(now.num_seconds_from_midnight())
        };
        self.next_at = slot.and_then(|(offset, second)| {
            let date = now
                .date_naive()
                .checked_add_signed(Duration::days(i64::from(offset)))?;
            let time = date.and_hms_opt(second / 3600, second / 60 % 60, second % 60)?;
            Local.from_local_datetime(&time).earliest()
        });
    }

    pub fn state(&self, now: DateTime<Local>) -> WaterState {
        let (consumed_ml, hourly_ml) = daily_totals(&self.drinks, now);
        WaterState {
            enabled: self.plan.enabled,
            consumed_ml,
            goal_ml: self.plan.goal_ml,
            bottle_ml: self.plan.bottle_ml,
            hourly_ml,
            active_from_minute: self.plan.active_from_minute,
            active_until_minute: self.plan.active_until_minute,
            next_at: self.next_at,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/water.rs"]
mod tests;
