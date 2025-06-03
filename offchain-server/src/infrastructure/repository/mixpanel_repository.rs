use candid::Principal;
use mixpanel_rs::Mixpanel;
use serde_json::Value;

use crate::domain::{errors::AppError, ports::analytics::AnalyticsRepository};

#[derive(Clone)]
pub struct MixpanelRepository {
    mixpanel: Mixpanel,
}

impl MixpanelRepository {
    pub fn new(project_token: String) -> Self {
        let mixpanel = Mixpanel::init(&project_token, None);
        Self { mixpanel }
    }
}

impl AnalyticsRepository for MixpanelRepository {
    async fn set_user(&self, payload: &mut Value) -> Result<Principal, AppError> {
        let principal = payload
            .get("principal")
            .and_then(|f| f.as_str())
            .map(str::to_owned);
        if principal.is_some() {
            let principal = Principal::from_text(principal.unwrap())?;
            payload["$user_id"] = principal.to_text().as_str().into();
            payload["distinct_id"] = principal.to_text().as_str().into();
            let mut user_payload = payload.clone();
            user_payload["$ip"] = payload["ip"].clone();
            let ip = payload["ip"].clone();
            let _ = self
                .mixpanel
                .people
                .set(&principal.to_text().as_str(), ip, user_payload)
                .await?;
            Ok(principal)
        } else {
            Err(AppError::InvalidData("Missing `principal` key".to_string()))
        }
    }

    async fn send(&self, event: &str, body: Value) -> Result<(), AppError> {
        let _ = self.mixpanel.track(event, Some(body)).await?;
        Ok(())
    }
}
