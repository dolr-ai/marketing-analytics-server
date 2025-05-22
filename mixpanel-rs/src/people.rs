use crate::types::Config;
use crate::{errors::MixpanelError, profile_helpers::ProfileHelpers};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct MixpanelPeople {
    helper: ProfileHelpers,
}

impl MixpanelPeople {
    pub fn new(token: &str, config: Arc<Config>) -> Self {
        Self {
            helper: ProfileHelpers::new(token, config, "/engage#profile-set"),
        }
    }

    pub async fn set(&self, distinct_id: &str, properties: Value) -> Result<Value, MixpanelError> {
        self.helper
            .send(json!({
                "$distinct_id": distinct_id,
                "$set": properties
            }))
            .await
    }

    pub async fn increment(
        &self,
        distinct_id: &str,
        properties: Value,
    ) -> Result<Value, MixpanelError> {
        self.helper
            .send(json!({
                "$distinct_id": distinct_id,
                "$add": properties
            }))
            .await
    }

    pub async fn append(
        &self,
        distinct_id: &str,
        properties: Value,
    ) -> Result<Value, MixpanelError> {
        self.helper
            .send(json!({
                "$distinct_id": distinct_id,
                "$append": properties
            }))
            .await
    }
}
