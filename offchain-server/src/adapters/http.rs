use anyhow::Context;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::*,
    Json, Router,
};
use candid::Principal;
use chrono::{DateTime, Utc};
use google_cloud_bigquery::http::tabledata::insert_all::{InsertAllRequest, Row};
use http::HeaderMap;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::net;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use woothee::parser::Parser;

use super::{
    app_state::AppState, auth_middleware::AuthenticatedRequest,
    sentry_webhook::sentry_webhook_handler,
};
use crate::{
    adapters::location_from_ip::insert_ip_details,
    application::services::mixpanel_analytics_service,
    config::Config,
    consts::{self, DEFAULT_OS},
    domain::errors::AppError,
    infrastructure::repository::mixpanel_repository::MixpanelRepository,
    ip_config::IpRange,
    utils::{classify_device, fetch_ip_details},
};
use axum::extract::ConnectInfo;

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
        pubsub_client: google_cloud_pubsub::client::Client,
        ip_client: Option<crate::ip_config::IpConfig>,
    ) -> anyhow::Result<Self> {
        let trace_layer =
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request<_>| {
                let uri = request.uri().to_string();
                tracing::info_span!("http_request", method = ?request.method(), uri)
            });

        // --- Create Pub/Sub Publisher once ---
        let pubsub_topic_name = consts::PUBSUB_TOPIC_NAME; // The topic you want to publish to
        let pubsub_topic = pubsub_client.topic(pubsub_topic_name);

        // Optional: Ensure topic exists on startup
        if !pubsub_topic.exists(None).await? {
            tracing::warn!(
                "Pub/Sub topic '{}' does not exist. Attempting to create it.",
                pubsub_topic_name
            );
            pubsub_topic.create(None, None).await.with_context(|| {
                format!("Failed to create Pub/Sub topic '{}'", pubsub_topic_name)
            })?;
            tracing::info!(
                "Successfully created Pub/Sub topic '{}'.",
                pubsub_topic_name
            );
        }

        let pubsub_event_publisher = Arc::new(pubsub_topic.new_publisher(None)); // Create it ONCE

        let state = AppState {
            config: env_config,
            bigquery_client,
            pubsub_event_publisher,
            pubsub_client: Arc::new(pubsub_client),
            analytics_service: Arc::new(analytics_service),
            ip_client: ip_client.map(Arc::new),
        };

        let router = Router::new()
            .route("/health", get(health_route))
            .route("/healthz", get(health_route))
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
        axum::serve(
            self.listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .context("received error from running server")?;
        Ok(())
    }
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/ip/{ip}", get(get_ip_range))
        .route("/my_ip", get(get_my_ip))
        .route("/btc_balance/{principal}", get(fetch_btc_balance))
        .route("/sats_balance/{principal}", get(fetch_sats_balance))
        .route("/is_canister_creator/{principal}", get(is_canister_creator))
        .route("/send_event", post(send_event_to_mixpanel))
        .route("/send_bigquery", post(send_event_to_bigquery))
        .route("/sentry", post(sentry_webhook_handler))
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

async fn is_canister_creator(Path(principal): Path<Principal>) -> Result<Json<bool>, AppError> {
    crate::utils::is_creator_canister(principal)
        .await
        .map(|f| Json(f))
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
    let ip_state = state.clone();
    let analytics = state.analytics_service;
    let principal = analytics.set_user(&mut payload).await?;
    let event = payload
        .get("event")
        .and_then(|f| f.as_str())
        .map(str::to_owned)
        .unwrap_or("unknown".into());
    let user_agent = payload
        .get("user_agent")
        .and_then(|f| f.as_str())
        .map(str::to_owned);
    let canister_id = payload
        .get("canister_id")
        .and_then(|f| f.as_str())
        .map(str::to_owned);
    if let Some(ua_lc) = user_agent {
        let parser = Parser::new();
        let os = parser.parse(&ua_lc).map(|f| f.os).unwrap_or(DEFAULT_OS);
        payload["$os"] = os.into();
        payload["device"] = classify_device(&ua_lc).into();
    }
    if let Ok(bal) = crate::utils::btc_balance_of(principal).await {
        payload["btc_balance_e8s"] = (bal as f64).into();
    }
    if let Ok(bal) = crate::utils::sats_balance_of(principal).await {
        payload["sats_balance"] = (bal).into();
    }
    if let Some(canister_id) = canister_id.map(|f| Principal::from_text(f).ok()).flatten() {
        if let Ok(is_creator) = crate::utils::is_creator_canister(canister_id).await {
            payload["is_creator"] = (is_creator).into();
        }
    }
    analytics.send(&event, payload.clone()).await?;
    send_to_bigquery(&ip_state, payload).await
}

async fn send_event_to_bigquery(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<Value>,
) -> Result<(), AppError> {
    let mut payload = payload;
    if payload.get("ip_addr").is_none() {
        let client_ip = headers
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(',').next()) // take first if multiple
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| addr.ip().to_string()); // fallback to socket addr
        payload["ip_addr"] = client_ip.into();
    }

    match payload.clone() {
        Value::Array(events) => {
            let futures = events
                .into_iter()
                .map(|event| send_to_bigquery(&state, event));
            let results: Vec<_> = futures::future::join_all(futures).await;
            for res in results {
                res?;
            }
            Ok(())
        }
        Value::Object(_) => send_to_bigquery(&state, payload).await,
        _ => Err(AppError::InvalidData(
            "Event payload must be an array or object".into(),
        )),
    }
}
async fn send_to_bigquery(state: &AppState, mut payload: Value) -> Result<(), AppError> {
    let ip = payload
        .get("ip_addr")
        .and_then(|f| f.as_str())
        .map(str::to_owned);
    let event = payload
        .get("event")
        .and_then(|f| f.as_str())
        .map(str::to_owned)
        .unwrap_or("unknown".into());
    if let Some(ip) = ip {
        if let Ok(res) = fetch_ip_details(&state, &ip) {
            let _ = insert_ip_details(res, &mut payload);
        }
    }
    let current_timestamp: DateTime<Utc> = Utc::now();
    let formatted_timestamp = current_timestamp.to_rfc3339();
    let pubsub_event_data = json!({ "timestamp": formatted_timestamp, "event_data": payload, });
    if let Ok(pubsub_message_data) =
        serde_json::to_string(&pubsub_event_data).map(|f| f.into_bytes())
    {
        let mut attributes: HashMap<String, String> = HashMap::new();
        attributes.insert("event_type".to_string(), event.clone());
        attributes.insert("source".to_string(), "analytics_server".to_string());
        let pubsub_message = google_cloud_googleapis::pubsub::v1::PubsubMessage {
            data: pubsub_message_data,
            attributes,
            message_id: String::new(),
            publish_time: None,
            ordering_key: String::new(),
        };
        let res = state.pubsub_event_publisher.publish(pubsub_message).await;
        match res.get().await {
            Ok(message_id) => {
                tracing::info!(
                    "Successfully published Pub/Sub message with ID: {}",
                    message_id
                );
            }
            Err(e) => {
                tracing::error!("Failed to publish Pub/Sub message: {:?}", e);
            }
        }
    }
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

async fn get_my_ip(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<String>, AppError> {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next()) // take first if multiple
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string()); // fallback to socket addr

    Ok(Json(client_ip))
}

async fn get_ip_range(
    _: AuthenticatedRequest,
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<IpRange>, AppError> {
    fetch_ip_details(&state, &ip).map(|f| Json(f))
}

async fn health_route() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}
