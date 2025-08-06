use std::{net::IpAddr, path::PathBuf};

use crate::domain::errors::AppError;
use crate::ip_config::IpRange;
use maxminddb::{geoip2, Reader};

pub struct Looker {
    reader: Reader<Vec<u8>>,
}

impl Looker {
    pub fn new(path: PathBuf) -> Result<Self, AppError> {
        let reader = Reader::open_readfile(path)
            .map_err(|e| AppError::IpConfigError(format!("Failed to open DB: {}", e)))?;

        Ok(Self { reader })
    }

    pub fn look_up(&self, ip: &str) -> Result<IpRange, AppError> {
        let ip: IpAddr = ip
            .parse()
            .map_err(|e| AppError::IpConfigError(format!("Invalid IP: {}", e)))?;

        // Use `lookup` directly — this returns Result<T, _>
        let city: geoip2::City = self
            .reader
            .lookup(ip)
            .map_err(|e| AppError::IpConfigError(format!("Lookup failed: {}", e)))?
            .ok_or_else(|| AppError::IpConfigError("No data found for IP".to_string()))?;

        let country = city
            .country
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en"))
            .cloned()
            .unwrap_or_default();

        let region = city
            .subdivisions
            .as_ref()
            .and_then(|subs| subs.first())
            .and_then(|sub| sub.names.as_ref())
            .and_then(|names| names.get("en"))
            .cloned()
            .unwrap_or_default();

        let city_name = city
            .city
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en"))
            .cloned()
            .unwrap_or_default();

        Ok(IpRange {
            country: country.into(),
            region: region.into(),
            city: city_name.into(),
        })
    }
}
