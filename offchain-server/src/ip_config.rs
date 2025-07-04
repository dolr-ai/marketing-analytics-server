use std::{path::PathBuf, str::FromStr};

use ip_check::{IpLookup, Looker};
use serde::{Deserialize, Serialize};

use crate::domain::errors::AppError;

pub struct IpConfig {
    looker: Looker,
}

#[derive(Serialize, Debug, Deserialize)]
pub struct IpRange {
    pub country: String,
    pub region: String,
    pub city: String,
}

impl IpConfig {
    pub fn load(path: &str) -> Result<Self, AppError> {
        let file_path = PathBuf::from_str(path)
            .map_err(|f| AppError::IpConfigError(format!("Invalid path: {}", f)))?;
        let looker = Looker::new(file_path);
        Ok(IpConfig { looker })
    }

    pub fn look_up(&self, ip: &str) -> Option<IpRange> {
        self.looker.look_up(ip).map(|f| IpRange {
            country: f.country,
            region: f.region,
            city: f.city,
        })
    }
}
