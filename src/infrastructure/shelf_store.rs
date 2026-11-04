pub(super) const SHELF_SCHEMA_VERSION: u32 = 2;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::task_store::write_atomically;
use crate::domain::ports::{PortError, ShelfRepository};
use crate::domain::shelf::{ShelfContent, ShelfItem, ShelfItemKind};

/// Persistence record for a zero-copy shelf reference.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShelfDocument {
    pub schema_version: u32,
    pub items: Vec<ShelfItem>,
}

/// Stores shelf item path references as JSON in `%APPDATA%\Lumi\shelf.json`.
pub struct JsonShelfRepository {
    path: PathBuf,
}

impl JsonShelfRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ShelfRepository for JsonShelfRepository {
    fn load(&self) -> Result<Vec<ShelfItem>, PortError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read(&self.path)
            .map_err(|error| PortError::new("shelf.error.storageRead", error))?;
        match serde_json::from_slice::<ShelfDocument>(&content) {
            Ok(document) => {
                if document.schema_version != SHELF_SCHEMA_VERSION
                    || document.items.iter().any(|item| match &item.content {
                        ShelfContent::Text(text) => {
                            text.trim().is_empty()
                                || text.contains('\0')
                                || item.kind != ShelfItemKind::Text
                                || item.is_directory
                        }
                        ShelfContent::File(path) => {
                            path.as_os_str().is_empty() || item.kind == ShelfItemKind::Text
                        }
                    })
                {
                    return Err(PortError::new(
                        "shelf.error.storageCorrupt",
                        "invalid shelf schema or content",
                    ));
                }
                Ok(document.items)
            }
            Err(error) => Err(PortError::new("shelf.error.storageCorrupt", error)),
        }
    }

    fn save(&self, items: &[ShelfItem]) -> Result<(), PortError> {
        let content = serde_json::to_vec_pretty(&ShelfDocument {
            schema_version: SHELF_SCHEMA_VERSION,
            items: items.to_vec(),
        })
        .map_err(|error| PortError::new("shelf.error.storageSerialize", error))?;
        write_atomically(&self.path, &content)
            .map_err(|error| PortError::new("shelf.error.storageWrite", error))
    }
}
