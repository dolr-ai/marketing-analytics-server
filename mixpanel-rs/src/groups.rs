use crate::types::Config;
use crate::{errors::MixpanelError, profile_helpers::ProfileHelpers};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct MixpanelGroups {
    helper: ProfileHelpers,
}

impl MixpanelGroups {
    pub fn new(token: &str, config: Arc<Config>) -> Self {
        Self {
            helper: ProfileHelpers::new(token, config, "/groups"),
        }
    }

    pub async fn set(
        &self,
        group_key: &str,
        group_id: &str,
        properties: Value,
    ) -> Result<Value, MixpanelError> {
        self.helper
            .send(json!({
                "$group_key": group_key,
                "$group_id": group_id,
                "$set": properties
            }))
            .await
    }

    pub async fn set_once(
        &self,
        group_key: &str,
        group_id: &str,
        properties: Value,
    ) -> Result<Value, MixpanelError> {
        self.helper
            .send(json!({
                "$group_key": group_key,
                "$group_id": group_id,
                "$set_once": properties
            }))
            .await
    }
}
