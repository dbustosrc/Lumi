const WATER_SCHEMA_VERSION: u32 = 1;

use super::task_store::write_atomically;
use crate::domain::ports::{PortError, WaterRepository};
use crate::domain::water::WaterDrink;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaterDocument {
    schema_version: u32,
    drinks: Vec<WaterDrink>,
}
pub struct JsonWaterRepository {
    path: PathBuf,
}
impl JsonWaterRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}
impl WaterRepository for JsonWaterRepository {
    fn load(&self) -> Result<Vec<WaterDrink>, PortError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content =
            std::fs::read(&self.path).map_err(|error| PortError::new("water.error.read", error))?;
        let document: WaterDocument = serde_json::from_slice(&content)
            .map_err(|error| PortError::new("water.error.parse", error))?;
        if document.schema_version != WATER_SCHEMA_VERSION
            || document.drinks.iter().any(|drink| drink.amount_ml == 0)
        {
            return Err(PortError::new("water.error.parse", "invalid water history"));
        }
        Ok(document.drinks)
    }
    fn save(&self, drinks: &[WaterDrink]) -> Result<(), PortError> {
        let content = serde_json::to_vec_pretty(&WaterDocument {
            schema_version: WATER_SCHEMA_VERSION,
            drinks: drinks.to_vec(),
        })
        .map_err(|error| PortError::new("water.error.write", error))?;
        write_atomically(&self.path, &content)
            .map_err(|error| PortError::new("water.error.write", error))
    }
}
