use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusRing {
    #[default]
    Auto,
    Water,
    Todo,
    Off,
}

impl StatusRing {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Water, Self::Todo, Self::Off];
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    #[default]
    Week,
    Water,
    Focus,
    WorldClock,
    Battery,
    Moon,
    Countdown,
}

impl Widget {
    pub const ALL: [Self; 7] = [
        Self::Week,
        Self::Water,
        Self::Focus,
        Self::WorldClock,
        Self::Battery,
        Self::Moon,
        Self::Countdown,
    ];
}

pub struct City {
    pub name: &'static str,
    pub zone: &'static str,
}

pub const CITIES: [City; 12] = [
    City {
        name: "Tokyo",
        zone: "Tokyo Standard Time",
    },
    City {
        name: "London",
        zone: "GMT Standard Time",
    },
    City {
        name: "Quito",
        zone: "SA Pacific Standard Time",
    },
    City {
        name: "New York",
        zone: "Eastern Standard Time",
    },
    City {
        name: "Los Angeles",
        zone: "Pacific Standard Time",
    },
    City {
        name: "Mexico City",
        zone: "Central Standard Time (Mexico)",
    },
    City {
        name: "São Paulo",
        zone: "E. South America Standard Time",
    },
    City {
        name: "Madrid",
        zone: "Romance Standard Time",
    },
    City {
        name: "Dubai",
        zone: "Arabian Standard Time",
    },
    City {
        name: "New Delhi",
        zone: "India Standard Time",
    },
    City {
        name: "Singapore",
        zone: "Singapore Standard Time",
    },
    City {
        name: "Sydney",
        zone: "AUS Eastern Standard Time",
    },
];

pub fn moon_phase(now: DateTime<Utc>) -> (usize, u8) {
    // Approximate phase from the 2000-01-06 18:14 UTC new moon and mean synodic month.
    const EPOCH: f64 = 947_182_440.;
    const LUNATION_SECONDS: f64 = 29.530_588_853 * 86_400.;
    let turn = ((now.timestamp() as f64 - EPOCH) / LUNATION_SECONDS).rem_euclid(1.);
    let illumination = (1. - (turn * std::f64::consts::TAU).cos()) * 50.;
    (
        ((turn * 8.).round() as usize) % 8,
        illumination.round() as u8,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FocusDay {
    pub date: NaiveDate,
    pub minutes: u64,
    pub sessions: u32,
}

impl FocusDay {
    pub fn empty(date: NaiveDate) -> Self {
        Self {
            date,
            minutes: 0,
            sessions: 0,
        }
    }

    pub fn for_date(self, date: NaiveDate) -> Self {
        if self.date == date {
            self
        } else {
            Self::empty(date)
        }
    }

    pub fn complete(self, date: NaiveDate, minutes: u64) -> Self {
        let mut day = self.for_date(date);
        day.minutes = day.minutes.saturating_add(minutes);
        day.sessions = day.sessions.saturating_add(1);
        day
    }
}
