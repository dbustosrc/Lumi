use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::domain::ports::PortError;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenInfo {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

pub struct IssuedToken {
    pub id: Uuid,
    pub secret: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenRecord {
    info: TokenInfo,
    digest: [u8; 32],
}

pub struct TaskTokens {
    path: PathBuf,
    records: Mutex<Vec<TokenRecord>>,
}

impl TaskTokens {
    pub fn load(path: PathBuf) -> Result<Self, PortError> {
        let records = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage_error)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(storage_error(error)),
        };
        Ok(Self {
            path,
            records: Mutex::new(records),
        })
    }

    pub fn list(&self) -> Vec<TokenInfo> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .map(|record| record.info)
            .collect()
    }

    pub fn generate(&self) -> Result<IssuedToken, PortError> {
        let mut random = [0; 32];
        getrandom::fill(&mut random)
            .map_err(|_| PortError::new("taskApi.error.random", "OS random generator failed"))?;
        let mut secret = String::with_capacity(69);
        secret.push_str("lumi_");
        for byte in random {
            write!(&mut secret, "{byte:02x}").expect("writing to String cannot fail");
        }
        let id = Uuid::new_v4();
        self.commit(|records| {
            records.push(TokenRecord {
                info: TokenInfo {
                    id,
                    created_at: Utc::now(),
                },
                digest: Sha256::digest(secret.as_bytes()).into(),
            });
        })?;
        Ok(IssuedToken { id, secret })
    }

    pub fn revoke(&self, id: Uuid) -> Result<(), PortError> {
        self.commit(|records| records.retain(|record| record.info.id != id))
    }

    pub fn authorizes(&self, secret: &str) -> bool {
        if secret.len() != 69 || !secret.starts_with("lumi_") {
            return false;
        }
        let digest: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        self.records
            .lock()
            .unwrap()
            .iter()
            .any(|record| bool::from(record.digest.ct_eq(&digest)))
    }

    fn commit(&self, change: impl FnOnce(&mut Vec<TokenRecord>)) -> Result<(), PortError> {
        let mut records = self.records.lock().unwrap();
        let mut next = records.clone();
        change(&mut next);
        let bytes = serde_json::to_vec(&next).map_err(storage_error)?;
        super::task_store::write_atomically(&self.path, &bytes).map_err(storage_error)?;
        *records = next;
        Ok(())
    }
}

fn storage_error(error: impl std::fmt::Display) -> PortError {
    PortError::new("taskApi.error.storage", error)
}
