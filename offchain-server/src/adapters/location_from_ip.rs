use std::env;

use reqwest;
use serde::Deserialize;
use serde_json::Value;

use crate::domain::errors::AppError;

#[derive(Deserialize, Debug)]
pub struct IpInfo {
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    loc: Option<String>,
    postal: Option<String>,
    timezone: Option<String>,
}

pub async fn insert_ip_details(payload: &mut Value) -> Result<(), AppError> {
    let ip_token = env::var("IP_TOKEN").ok();
    let ip = payload
        .get("ip_addr")
        .and_then(|f| f.as_str())
        .map(str::to_owned);
    if let Some(ip) = ip {
        let url = format!("https://ipinfo.io/{}/json", ip);

        let url = if let Some(token) = ip_token {
            format!("{}?token={}", url, token)
        } else {
            url
        };

        let response = reqwest::get(&url).await?;
        let ip_info: IpInfo = response.json().await?;
        if let Some(city) = ip_info.city {
            payload["city"] = city.into();
        }
        if let Some(region) = ip_info.region {
            payload["region"] = region.into();
        }
        if let Some(country) = ip_info.country {
            payload["country"] = country.into();
        }
        if let Some(loc) = ip_info.loc {
            let coords: Vec<&str> = loc.split(',').map(str::trim).collect();
            if coords.len() == 2 {
                payload["latitude"] = coords[0].into();
                payload["longitude"] = coords[1].into();
            }
        }
        if let Some(postal) = ip_info.postal {
            payload["postal"] = postal.into();
        }
        if let Some(timezone) = ip_info.timezone {
            payload["timezone"] = timezone.into();
        }
    }
    Ok(())
}
