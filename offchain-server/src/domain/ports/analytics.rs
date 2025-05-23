use crate::domain::errors::AppError;
use candid::Principal;
use serde_json::Value;
use std::future::Future;

pub trait AnalyticsRepository: Send + Sync + 'static {
    fn set_user(
        &self,
        payload: &mut Value,
    ) -> impl Future<Output = Result<Principal, AppError>> + Send;
    fn send(
        &self,
        event: &str,
        payload: Value,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}
