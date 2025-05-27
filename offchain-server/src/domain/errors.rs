use std::num::ParseIntError;

use axum::{http::StatusCode, response::IntoResponse};
use ic_agent::{AgentError, export::PrincipalError};
use mixpanel_rs::errors::MixpanelError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("API error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Mixpanel error: {0}")]
    MixpanelError(#[from] MixpanelError),
    #[error("Invalid data {0}")]
    InvalidData(String),
    #[error("Invalid principal {0}")]
    PrincipalError(#[from] PrincipalError),
    #[error("IC agent error {0}")]
    IcAgentError(#[from] AgentError),
    #[error("Failed to parse Int from Nat {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("Decode error {0}")]
    CandidError(#[from] candid::Error),
    #[error("Bigquery error {0}")]
    BigqueryError(#[from] google_cloud_bigquery::http::error::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::Unauthorized(_) => {
                (StatusCode::UNAUTHORIZED, self.to_string()).into_response()
            }
            AppError::InvalidData(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            AppError::ReqwestError(e) => (
                e.status()
                    .map(|f| axum::http::StatusCode::from_u16(f.as_u16()))
                    .unwrap_or(Ok(StatusCode::BAD_REQUEST))
                    .unwrap(),
                e.to_string(),
            )
                .into_response(),
            _ => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
        }
    }
}
