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
    offset: chrono::FixedOffset,
    totals: (u64, [u64; 24]),
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
        let totals = daily_totals(&drinks, now);
        let mut service = Self {
            repository,
            drinks,
            plan,
            next_at: None,
            day: now.date_naive(),
            offset: *now.offset(),
            totals,
        };
        service.reschedule(now);
        Ok(service)
    }

    pub fn configure(&mut self, plan: &WaterPlan, now: DateTime<Local>) {
        let day_changed = self.roll_day(now);
        if self.plan != *plan || day_changed {
            self.plan = plan.clone();
            self.reschedule(now);
        }
    }

    pub fn drink(&mut self, now: DateTime<Local>) -> Result<bool, PortError> {
        if self.roll_day(now) {
            self.reschedule(now);
        }
        let amount = self.plan.dose_ml();
        let mut next = self.drinks.clone();
        next.push(WaterDrink {
            at: now.fixed_offset(),
            amount_ml: amount,
        });
        self.repository.save(&next)?;
        self.drinks = next;
        self.totals.0 += u64::from(amount);
        self.totals.1[now.hour() as usize] += u64::from(amount);
        self.reschedule(now);
        Ok(true)
    }

    pub fn reset_today(&mut self, now: DateTime<Local>) -> Result<bool, PortError> {
        if self.roll_day(now) {
            self.reschedule(now);
        }
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
        self.totals = (0, [0; 24]);
        self.reschedule(now);
        Ok(true)
    }

    pub fn tick(&mut self, now: DateTime<Local>) -> bool {
        let expired = self.next_at.is_some_and(|at| now >= at);
        if self.roll_day(now) && !expired {
            self.reschedule(now);
        }
        let second = now.num_seconds_from_midnight();
        let due = expired
            && self.plan.enabled
            && second >= u32::from(self.plan.active_from_minute) * 60
            && second < u32::from(self.plan.active_until_minute) * 60
            && self.totals.0 < u64::from(self.plan.goal_ml);
        if expired {
            self.reschedule(now);
        }
        due
    }

    fn roll_day(&mut self, now: DateTime<Local>) -> bool {
        if self.day == now.date_naive() && self.offset == *now.offset() {
            return false;
        }
        self.day = now.date_naive();
        self.offset = *now.offset();
        self.totals = daily_totals(&self.drinks, now);
        true
    }

    pub fn deadline(&self) -> Option<DateTime<Local>> {
        self.next_at
    }

    fn reschedule(&mut self, now: DateTime<Local>) {
        let consumed = self.totals.0;
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
        let (consumed_ml, hourly_ml) =
            if now.date_naive() == self.day && *now.offset() == self.offset {
                self.totals
            } else {
                daily_totals(&self.drinks, now)
            };
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
