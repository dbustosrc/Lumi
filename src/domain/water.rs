use chrono::{DateTime, FixedOffset, Local, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterPlan {
    pub enabled: bool,
    pub goal_ml: u16,
    pub bottle_ml: u16,
    pub active_from_minute: u16,
    pub active_until_minute: u16,
}

impl Default for WaterPlan {
    fn default() -> Self {
        Self {
            enabled: false,
            goal_ml: 1500,
            bottle_ml: 500,
            active_from_minute: 480,
            active_until_minute: 1020,
        }
    }
}

impl WaterPlan {
    pub fn reminder_count(&self) -> u32 {
        u32::from(
            self.active_until_minute
                .saturating_sub(self.active_from_minute),
        )
        .div_ceil(15)
        .max(1)
    }

    pub fn dose_ml(&self) -> u16 {
        u32::from(self.goal_ml).div_ceil(self.reminder_count()) as u16
    }

    pub fn sips_per_bottle(&self) -> u32 {
        u32::from(self.bottle_ml).div_ceil(u32::from(self.dose_ml()).max(1))
    }

    pub fn refills(&self) -> u32 {
        u32::from(self.goal_ml)
            .div_ceil(u32::from(self.bottle_ml).max(1))
            .saturating_sub(1)
    }
    pub fn is_valid(&self) -> bool {
        self.goal_ml > 0
            && self.bottle_ml > 0
            && self.active_from_minute < self.active_until_minute
            && self.active_until_minute <= 1440
    }

    /// Evenly spaced doses inside [from, until); the first dose is at the start.
    pub fn next_slot(&self, second_of_day: u32) -> Option<(u32, u32)> {
        if !self.enabled || !self.is_valid() {
            return None;
        }
        let start = u32::from(self.active_from_minute) * 60;
        let end = u32::from(self.active_until_minute) * 60;
        let doses = self.reminder_count();
        for index in 0..doses {
            let slot = start + (end - start) * index / doses;
            if slot > second_of_day {
                return Some((0, slot));
            }
        }
        Some((1, start))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterDrink {
    pub at: DateTime<FixedOffset>,
    pub amount_ml: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterState {
    pub enabled: bool,
    pub consumed_ml: u64,
    pub goal_ml: u16,
    pub bottle_ml: u16,
    pub hourly_ml: [u64; 24],
    pub active_from_minute: u16,
    pub active_until_minute: u16,
    pub next_at: Option<DateTime<Local>>,
}

pub fn daily_totals(drinks: &[WaterDrink], now: DateTime<Local>) -> (u64, [u64; 24]) {
    let mut hourly = [0_u64; 24];
    for drink in drinks {
        let at = drink.at.with_timezone(&Local);
        if at.date_naive() == now.date_naive() {
            hourly[at.hour() as usize] += u64::from(drink.amount_ml);
        }
    }
    (hourly.iter().sum(), hourly)
}
