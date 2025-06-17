use std::env;

use reqwest;
use serde::Deserialize;
use serde_json::Value;

use crate::domain::errors::AppError;

#[derive(Deserialize, Debug)]
struct GeoName {
    names: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize, Debug)]
struct Location {
    latitude: Option<f64>,
    longitude: Option<f64>,
    #[serde(rename = "time_zone")]
    time_zone: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Traits {
    isp: Option<String>,
}

#[derive(Deserialize, Debug)]
struct IpInfo {
    city: Option<GeoName>,
    country: Option<GeoName>,
    subdivisions: Option<Vec<GeoName>>,
    location: Option<Location>,
    traits: Option<Traits>,
}

pub async fn insert_ip_details(payload: &mut Value) -> Result<(), AppError> {
    let ip_token = env::var("IP_TOKEN").ok();
    let ip = payload
        .get("ip_addr")
        .and_then(|f| f.as_str())
        .map(str::to_owned);

    if let Some(ip) = ip {
        let url = if let Some(token) = ip_token {
            format!("https://api.findip.net/{}/?token={}", ip, token)
        } else {
            format!("https://api.findip.net/{}/", ip)
        };

        let response = reqwest::get(&url).await?;
        let ip_info: IpInfo = response.json().await?;

        if let Some(city) = ip_info.city {
            if let Some(name) = city.names.and_then(|m| m.get("en").cloned()) {
                payload["city"] = name.into();
            }
        }

        if let Some(subdivisions) = ip_info.subdivisions {
            if let Some(region) = subdivisions.first() {
                if let Some(name) = region.names.as_ref().and_then(|m| m.get("en")) {
                    payload["region"] = name.clone().into();
                }
            }
        }

        if let Some(country) = ip_info.country {
            if let Some(name) = country.names.and_then(|m| m.get("en").cloned()) {
                payload["country"] = name.into();
            }
        }

        if let Some(location) = ip_info.location {
            if let Some(lat) = location.latitude {
                payload["latitude"] = lat.into();
            }
            if let Some(lon) = location.longitude {
                payload["longitude"] = lon.into();
            }
            if let Some(timezone) = location.time_zone {
                payload["timezone"] = timezone.into();
            }
        }

        if let Some(traits) = ip_info.traits {
            if let Some(isp) = traits.isp {
                payload["isp"] = isp.into();
            }
        }
    }

    Ok(())
}
