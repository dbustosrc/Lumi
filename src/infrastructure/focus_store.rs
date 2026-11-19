use std::{fs, io, path::PathBuf};

use chrono::NaiveDate;

use super::task_store::write_atomically;
use crate::application::controller::FocusDayStore;
use crate::domain::{ports::PortError, widgets::FocusDay};

pub struct JsonFocusDayStore(pub PathBuf);

impl JsonFocusDayStore {
    pub fn load(&self, today: NaiveDate) -> Result<FocusDay, PortError> {
        match fs::read(&self.0) {
            Ok(bytes) => serde_json::from_slice::<FocusDay>(&bytes)
                .map(|day| day.for_date(today))
                .map_err(|e| PortError::new("focus.error.read", e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(FocusDay::empty(today)),
            Err(e) => Err(PortError::new("focus.error.read", e)),
        }
    }
}

impl FocusDayStore for JsonFocusDayStore {
    fn save(&self, day: FocusDay) -> Result<(), PortError> {
        let bytes = serde_json::to_vec(&day).map_err(|e| PortError::new("focus.error.write", e))?;
        write_atomically(&self.0, &bytes).map_err(|e| PortError::new("focus.error.write", e))
    }
}
