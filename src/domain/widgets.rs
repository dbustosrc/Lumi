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

pub const CITIES: &[City] = &[
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
    City {
        name: "Vancouver",
        zone: "Pacific Standard Time",
    },
    City {
        name: "Toronto",
        zone: "Eastern Standard Time",
    },
    City {
        name: "Halifax",
        zone: "Atlantic Standard Time",
    },
    City {
        name: "Chicago",
        zone: "Central Standard Time",
    },
    City {
        name: "Denver",
        zone: "Mountain Standard Time",
    },
    City {
        name: "Phoenix",
        zone: "US Mountain Standard Time",
    },
    City {
        name: "Anchorage",
        zone: "Alaskan Standard Time",
    },
    City {
        name: "Honolulu",
        zone: "Hawaiian Standard Time",
    },
    City {
        name: "Guatemala City",
        zone: "Central America Standard Time",
    },
    City {
        name: "Havana",
        zone: "Cuba Standard Time",
    },
    City {
        name: "Bogotá",
        zone: "SA Pacific Standard Time",
    },
    City {
        name: "Lima",
        zone: "SA Pacific Standard Time",
    },
    City {
        name: "Caracas",
        zone: "Venezuela Standard Time",
    },
    City {
        name: "La Paz",
        zone: "SA Western Standard Time",
    },
    City {
        name: "Santiago",
        zone: "Pacific SA Standard Time",
    },
    City {
        name: "Buenos Aires",
        zone: "Argentina Standard Time",
    },
    City {
        name: "Montevideo",
        zone: "Montevideo Standard Time",
    },
    City {
        name: "Rio de Janeiro",
        zone: "E. South America Standard Time",
    },
    City {
        name: "Reykjavík",
        zone: "Greenwich Standard Time",
    },
    City {
        name: "Dublin",
        zone: "GMT Standard Time",
    },
    City {
        name: "Lisbon",
        zone: "GMT Standard Time",
    },
    City {
        name: "Paris",
        zone: "Romance Standard Time",
    },
    City {
        name: "Berlin",
        zone: "W. Europe Standard Time",
    },
    City {
        name: "Amsterdam",
        zone: "W. Europe Standard Time",
    },
    City {
        name: "Rome",
        zone: "W. Europe Standard Time",
    },
    City {
        name: "Stockholm",
        zone: "W. Europe Standard Time",
    },
    City {
        name: "Warsaw",
        zone: "Central European Standard Time",
    },
    City {
        name: "Athens",
        zone: "GTB Standard Time",
    },
    City {
        name: "Kyiv",
        zone: "FLE Standard Time",
    },
    City {
        name: "Istanbul",
        zone: "Turkey Standard Time",
    },
    City {
        name: "Moscow",
        zone: "Russian Standard Time",
    },
    City {
        name: "Casablanca",
        zone: "Morocco Standard Time",
    },
    City {
        name: "Cairo",
        zone: "Egypt Standard Time",
    },
    City {
        name: "Nairobi",
        zone: "E. Africa Standard Time",
    },
    City {
        name: "Johannesburg",
        zone: "South Africa Standard Time",
    },
    City {
        name: "Jerusalem",
        zone: "Israel Standard Time",
    },
    City {
        name: "Riyadh",
        zone: "Arab Standard Time",
    },
    City {
        name: "Karachi",
        zone: "Pakistan Standard Time",
    },
    City {
        name: "Kathmandu",
        zone: "Nepal Standard Time",
    },
    City {
        name: "Dhaka",
        zone: "Bangladesh Standard Time",
    },
    City {
        name: "Bangkok",
        zone: "SE Asia Standard Time",
    },
    City {
        name: "Jakarta",
        zone: "SE Asia Standard Time",
    },
    City {
        name: "Beijing",
        zone: "China Standard Time",
    },
    City {
        name: "Hong Kong",
        zone: "China Standard Time",
    },
    City {
        name: "Taipei",
        zone: "Taipei Standard Time",
    },
    City {
        name: "Manila",
        zone: "Singapore Standard Time",
    },
    City {
        name: "Seoul",
        zone: "Korea Standard Time",
    },
    City {
        name: "Perth",
        zone: "W. Australia Standard Time",
    },
    City {
        name: "Adelaide",
        zone: "Cen. Australia Standard Time",
    },
    City {
        name: "Brisbane",
        zone: "E. Australia Standard Time",
    },
    City {
        name: "Melbourne",
        zone: "AUS Eastern Standard Time",
    },
    City {
        name: "Auckland",
        zone: "New Zealand Standard Time",
    },
    City {
        name: "Suva",
        zone: "Fiji Standard Time",
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
