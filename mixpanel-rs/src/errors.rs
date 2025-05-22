use thiserror::Error;

#[derive(Debug, Error)]
pub enum MixpanelError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Mixpanel returned error ({status}): {body}")]
    ApiError {
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("Unexpected error: {0}")]
    Other(String),
}
