use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{request::Parts, StatusCode},
};

use super::app_state::AppState;

pub struct AuthenticatedRequest;

impl<S> FromRequestParts<S> for AuthenticatedRequest
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {

        let State(state): State<AppState> = State::from_request_parts(parts, state).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Unauthorized"))?;

        let expected_token = state.config.server_access_token;

        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(header) if header == format!("Bearer {}", expected_token) => {
                Ok(AuthenticatedRequest)
            }
            _ => Err((StatusCode::UNAUTHORIZED, "Unauthorized")),
        }
    }
}
