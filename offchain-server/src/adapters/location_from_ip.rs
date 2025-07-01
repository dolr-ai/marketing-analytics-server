use serde_json::Value;

use crate::{domain::errors::AppError, ip_config::IpRange};

pub fn insert_ip_details(ip_range: IpRange, payload: &mut Value) -> Result<(), AppError> {
    payload["city"] = ip_range.city.into();
    payload["country"] = ip_range.country.into();
    payload["region"] = ip_range.region.into();
    Ok(())
}
