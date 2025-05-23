use candid::Principal;
use serde_json::Value;

use crate::domain::{errors::AppError, ports::analytics::AnalyticsRepository};

#[derive(Clone)]
pub struct MixpanelService<R: AnalyticsRepository> {
    repo: R,
}

impl<R: AnalyticsRepository> MixpanelService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
    pub async fn set_user(&self, payload: &mut Value) -> Result<Principal, AppError> {
        self.repo.set_user(payload).await
    }
    pub async fn send(&self, event: &str, payload: Value) -> Result<(), AppError> {
        self.repo.send(event, payload).await
    }
}
