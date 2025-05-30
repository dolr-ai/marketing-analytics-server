use reqwest::Client;
use serde::Deserialize;

use crate::domain::errors::AppError;

#[derive(Deserialize)]
pub struct IpApiResponse {
    pub city: Option<String>,
    pub country: Option<String>,
}

pub async fn lookup_ip(ip: &str) -> Result<IpApiResponse, AppError> {
    let url = format!("http://ip-api.com/json/{ip}?fields=status,city,country");
    let resp: IpApiResponse = Client::new()
        .get(&url)
        .send()
        .await?
        .json::<IpApiResponse>()
        .await?;
    Ok(resp)
}
