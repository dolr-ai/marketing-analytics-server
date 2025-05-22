use crate::utils::send_request;
use crate::{errors::MixpanelError, types::Config};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct ProfileHelpers {
    pub token: String,
    pub config: Arc<Config>,
    pub endpoint: String,
}

impl ProfileHelpers {
    pub fn new(token: &str, config: Arc<Config>, endpoint: &str) -> Self {
        Self {
            token: token.to_string(),
            config,
            endpoint: endpoint.to_string(),
        }
    }

    pub async fn send(&self, mut data: Value) -> Result<Value, MixpanelError> {
        data["$token"] = json!(self.token);
        send_request(&self.config, &self.endpoint, data).await
    }
}
