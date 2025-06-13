use axum::{body::Bytes, extract::State, http::HeaderMap, response::IntoResponse};
use hmac::{Hmac, Mac};
use http::StatusCode;
use k256::sha2::Sha256;
use serde::{Deserialize, Serialize};
use std::{env, sync::Arc};

use crate::application::services::sentry_service::SentryService;

use super::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct SentryWebhookPayload {
    pub data: Option<SentryData>,
}

#[derive(Debug, Deserialize)]
pub struct SentryData {
    pub event: Option<SentryEvent>,
}

#[derive(Debug, Deserialize)]
pub struct SentryEvent {
    pub web_url: Option<String>,
    pub title: Option<String>,
    pub user: Option<SentryUser>,
    pub level: Option<String>,
    pub platform: Option<String>,
    pub timestamp: Option<f64>,
    pub project: Option<u32>,
    pub logger: Option<String>,
    pub release: Option<String>,
    pub culprit: Option<String>,
    pub tags: Option<Vec<[String; 2]>>,
}

#[derive(Debug, Deserialize)]
pub struct SentryUser {
    pub id: Option<String>,
}

async fn verify_sentry_signature(headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode> {
    // Get the signature from headers
    let expected_signature = headers
        .get("sentry-hook-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Get the client secret from environment
    let client_secret =
        env::var("SENTRY_CLIENT_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create HMAC-SHA256
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(client_secret.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    mac.update(body);
    let digest = mac.finalize();
    let computed_signature = hex::encode(digest.into_bytes());

    // Compare signatures using constant-time comparison
    if computed_signature != expected_signature {
        tracing::warn!("Sentry webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

pub async fn sentry_webhook_handler(
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    tracing::info!("Sentry webhook received");

    // Verify signature
    if let Err(status) = verify_sentry_signature(&headers, &body).await {
        return Err((status, "Signature verification failed".to_string()));
    }

    tracing::info!("Sentry webhook signature verified");

    // Parse the JSON payload
    let payload: SentryWebhookPayload = serde_json::from_slice(&body).map_err(|e| {
        tracing::error!("Invalid JSON: {}", e);
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid JSON: {} {}", e, String::from_utf8_lossy(&body)),
        )
    })?;

    // Extract event data
    let event_data = payload.data.as_ref().and_then(|data| data.event.as_ref());

    if let Some(event) = event_data {
        let sentry_service = SentryService::new();

        if let Err(e) = sentry_service.process_webhook_event(event).await {
            tracing::error!("Failed to process Sentry webhook event: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to process event".to_string(),
            ));
        }
    }

    Ok(StatusCode::OK)
}
