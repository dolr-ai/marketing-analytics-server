use crate::errors::MixpanelError;
use crate::types::Config;
use reqwest::Client;
use serde_json::{json, Value};

#[cfg(feature = "tracing")]
use tracing::{debug, error, info, instrument};

#[cfg_attr(feature = "tracing", instrument(skip(config, payload)))]
pub async fn send_request(
    config: &Config,
    endpoint: &str,
    payload: Value,
) -> Result<Value, MixpanelError> {
    let client = Client::new();
    let url = format!("{}://{}{}", config.protocol, config.host, endpoint);
    let payload = json!([payload]);
    #[cfg(feature = "tracing")]
    debug!(%url, body = ?payload, "Sending request to Mixpanel");

    let res = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/plain")
        .json(&payload)
        .send()
        .await?;

    let status = res.status();
    let body = res
        .text()
        .await
        .unwrap_or_else(|_| "<could not read body>".into());

    if status.is_success() {
        #[cfg(feature = "tracing")]
        info!(status = ?status, payload = %payload,  body = %body, "Mixpanel request successful");
        Ok(payload)
    } else {
        #[cfg(feature = "tracing")]
        error!(status = ?status, body = %body, "Mixpanel API returned error");
        Err(MixpanelError::ApiError { status, body })
    }
}
