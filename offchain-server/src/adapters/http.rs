use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::*,
    Json, Router,
};
use candid::Principal;
use google_cloud_bigquery::http::tabledata::insert_all::{InsertAllRequest, Row};
use serde::Serialize;
use serde_json::Value;
use tokio::net;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    application::services::mixpanel_analytics_service, config::Config, domain::errors::AppError,
    infrastructure::repository::mixpanel_repository::MixpanelRepository,
};

use super::{app_state::AppState, auth_middleware::AuthenticatedRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerConfig<'a> {
    pub port: &'a str,
}

pub struct HttpServer {
    router: Router,
    listener: net::TcpListener,
}

impl HttpServer {
    pub async fn new(
        config: HttpServerConfig<'_>,
        env_config: Config,
        analytics_service: mixpanel_analytics_service::MixpanelService<MixpanelRepository>,
        bigquery_client: google_cloud_bigquery::client::Client,
    ) -> anyhow::Result<Self> {
        let trace_layer =
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request<_>| {
                let uri = request.uri().to_string();
                tracing::info_span!("http_request", method = ?request.method(), uri)
            });

        let state = AppState {
            config: env_config,
            bigquery_client,
            analytics_service: Arc::new(analytics_service),
        };

        let router = Router::new()
            .route("/health", get(health_route))
            .nest("/api", api_routes())
            .layer(trace_layer)
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = SocketAddr::from((
            [0, 0, 0, 0, 0, 0, 0, 0],
            config.port.parse::<u16>().unwrap_or(3000),
        ));

        let listener = net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("failed to listen on port {}", config.port))?;

        Ok(Self { router, listener })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        tracing::debug!("listening on {}", self.listener.local_addr().unwrap());
        axum::serve(self.listener, self.router)
            .await
            .context("received error from running server")?;
        Ok(())
    }
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/btc_balance/{principal}", get(fetch_btc_balance))
        .route("/sats_balance/{principal}", get(fetch_sats_balance))
        .route("/send_event", post(send_event_to_mixpanel))
}

#[derive(serde::Serialize)]
struct Balance {
    balance: f64,
}

async fn fetch_btc_balance(Path(principal): Path<Principal>) -> Result<Json<Balance>, AppError> {
    match crate::utils::btc_balance_of(principal).await {
        Ok(bal) => {
            let balance = bal as f64 / 100_000_000.0;
            Ok(Json(Balance { balance }))
        }
        Err(e) => Err(e),
    }
}

async fn fetch_sats_balance(Path(principal): Path<Principal>) -> Result<Json<Balance>, AppError> {
    match crate::utils::sats_balance_of(principal).await {
        Ok(balance) => Ok(Json(Balance { balance })),
        Err(e) => Err(e),
    }
}

#[derive(Serialize)]
struct BigQueryEvent {
    event: String,
    params: String,
    timestamp: String,
}
async fn send_event_to_mixpanel(
    _: AuthenticatedRequest,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<(), AppError> {
    let mut payload = payload;
    let analytics = state.analytics_service;
    let principal = analytics.set_user(&mut payload).await?;
    let event = payload
        .get("event")
        .and_then(|f| f.as_str())
        .map(str::to_owned)
        .unwrap_or("unknown".into());
    match crate::utils::btc_balance_of(principal).await {
        Ok(bal) => {
            payload["btc_balance_e8s"] = (bal as f64).into();
        }
        Err(_) => {}
    }
    match crate::utils::sats_balance_of(principal).await {
        Ok(bal) => {
            payload["sats_balance"] = (bal).into();
        }
        Err(_) => {}
    }
    match crate::utils::sats_balance_of(principal).await {
        Ok(bal) => {
            payload["sats_balance"] = (bal).into();
        }
        Err(_) => {}
    }
    analytics.send(&event, payload.clone()).await?;
    let payload = serde_json::to_string(&payload).unwrap();
    let row = Row {
        insert_id: None,
        json: BigQueryEvent {
            event: format!("mp_{event}"),
            params: payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    };
    let request = InsertAllRequest {
        rows: vec![row],
        ..Default::default()
    };
    let res = state
        .bigquery_client
        .tabledata()
        .insert(
            "hot-or-not-feed-intelligence",
            "analytics_335143420",
            "test_events_analytics",
            &request,
        )
        .await?;
    println!("BigQuery insert response: {:?}", res);
    Ok(())
}

async fn health_route() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}
