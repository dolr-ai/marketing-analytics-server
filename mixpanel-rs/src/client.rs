use crate::groups::MixpanelGroups;
use crate::types::Config;
use crate::utils::send_request;
use crate::{errors::MixpanelError, people::MixpanelPeople};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct Mixpanel {
    pub token: String,
    pub config: Arc<Config>,
    pub people: MixpanelPeople,
    pub groups: MixpanelGroups,
}

impl Mixpanel {
    pub fn init(token: &str, config: Option<Config>) -> Self {
        let config = Arc::new(config.unwrap_or_default());
        Self {
            token: token.to_string(),
            people: MixpanelPeople::new(token, config.clone()),
            groups: MixpanelGroups::new(token, config.clone()),
            config,
        }
    }

    pub async fn track(
        &self,
        event: &str,
        properties: Option<Value>,
    ) -> Result<Value, MixpanelError> {
        let mut props = properties.unwrap_or_default();
        props["token"] = json!(self.token);
        // Timestamp
        if !props.get("time").is_some() {
            props["time"] = json!(chrono::Utc::now().timestamp());
        }

        // Insert ID
        if !props.get("$insert_id").is_some() {
            props["$insert_id"] = json!(uuid::Uuid::new_v4().to_string());
        }

        let body = json!({
            "event": event,
            "properties": props,
        });
        send_request(&self.config, "/track", body).await
    }

    pub async fn alias(&self, distinct_id: &str, alias: &str) -> Result<Value, MixpanelError> {
        let props = json!({
            "distinct_id": distinct_id,
            "alias": alias,
            "token": self.token
        });
        self.track("$create_alias", Some(props)).await
    }

    pub async fn identify(&self, old_id: &str, new_id: &str) -> Result<Value, MixpanelError> {
        let props = json!({
            "$identified_id": new_id,
            "$anon_id": old_id,
            "token": self.token
        });
        let data = json!({
            "event": "$identify",
            "properties": props,
        });
        send_request(&self.config, "/track", data).await
    }

    pub async fn import(
        &self,
        event: &str,
        time: i64,
        mut properties: Value,
    ) -> Result<Value, MixpanelError> {
        properties["time"] = json!(time);
        properties["token"] = json!(self.token);
        let body = json!({ "event": event, "properties": properties });
        send_request(&self.config, "/import", body).await
    }
}
