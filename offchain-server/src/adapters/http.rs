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
use serde::{Deserialize, Serialize};
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
    ip_config::{IpRange, IpRangeV2},
    utils::{classify_device, fetch_ip_details, fetch_ip_details_v2},
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
        .route("/ip_v2/{ip}", get(get_ip_range_v2))
        .route("/my_ip", get(get_my_ip))
        .route("/my_timezone", get(get_my_timezone))
        .route("/btc_balance/{principal}", get(fetch_btc_balance))
        .route("/sats_balance/{principal}", get(fetch_sats_balance))
        .route("/send_event", post(send_event_to_mixpanel))
        .route("/send_bigquery", post(send_event_to_bigquery))
        .route("/sentry", post(sentry_webhook_handler))
}

#[derive(serde::Serialize)]
struct Balance {
    balance: f64,
}

#[derive(serde::Serialize)]
struct TimezoneInfo {
    timezone: String,
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

/// Structure for individual event data within a bulk event
#[derive(Debug, Clone, Deserialize, Serialize)]
struct EventData {
    #[serde(flatten)]
    fields: HashMap<String, Value>,
}

/// Structure for a row in bulk events from mobile clients
#[derive(Debug, Clone, Deserialize, Serialize)]
struct EventRow {
    event_data: EventData,
}

/// Structure for bulk events payload from mobile clients
/// Format: { "event_data": { "city": "", "country": "", "ip_addr": "", "rows": [...] } }
#[derive(Debug, Clone, Deserialize, Serialize)]
struct BulkEventData {
    #[serde(flatten)]
    common_fields: HashMap<String, Value>,
    rows: Vec<EventRow>,
}

/// Enum to represent different event payload types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum EventPayload {
    /// Bulk event with nested structure from mobile clients
    Bulk(BulkEventData),
    /// Array of individual events
    Array(Vec<Value>),
    /// Single event object
    Single(Value),
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
        if let Ok(is_creator) = crate::utils::is_creator(principal, canister_id).await {
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
    Json(payload): Json<EventPayload>,
) -> Result<(), AppError> {
    // Extract IP address from headers if not present
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    match payload {
        EventPayload::Bulk(bulk_payload) => {
            // Handle nested bulk event structure from mobile team
            let common_fields = bulk_payload.common_fields;

            // Process each event in rows, merging with common fields
            let futures = bulk_payload.rows.iter().map(|row| {
                // Merge common fields with event fields (event fields take precedence)
                let mut merged = common_fields.clone();

                // Add IP address if not present in common fields
                merged
                    .entry("ip_addr".to_string())
                    .or_insert_with(|| Value::String(client_ip.clone()));

                // Extend with event-specific fields
                merged.extend(row.event_data.fields.clone());

                tracing::info!("Inserting single row  from bulk data {merged:?}",);

                send_to_bigquery(&state, Value::Object(merged.into_iter().collect()))
            });

            let results: Vec<_> = futures::future::join_all(futures).await;
            for res in results {
                res?;
            }
            Ok(())
        }
        EventPayload::Array(events) => {
            // Handle array of events
            tracing::info!("Recieved Array of events from bulk data {events:?}",);
            let futures = events.into_iter().map(|mut event| {
                // Add IP address if not present
                if let Some(obj) = event.as_object_mut() {
                    obj.entry("ip_addr".to_string())
                        .or_insert_with(|| Value::String(client_ip.clone()));
                }
                send_to_bigquery(&state, event)
            });

            let results: Vec<_> = futures::future::join_all(futures).await;
            for res in results {
                res?;
            }
            Ok(())
        }
        EventPayload::Single(mut event) => {
            // Handle single event
            if let Some(obj) = event.as_object_mut() {
                obj.entry("ip_addr".to_string())
                    .or_insert_with(|| Value::String(client_ip.clone()));
            }
            tracing::info!("Recieved single payload from bulk data {event:?}",);
            send_to_bigquery(&state, event).await
        }
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

async fn get_my_timezone(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<TimezoneInfo>, AppError> {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next()) // take first if multiple
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string()); // fallback to socket addr

    let ip_info = fetch_ip_details_v2(&state, &client_ip)?;

    Ok(Json(TimezoneInfo {
        timezone: ip_info.timezone,
    }))
}

async fn get_ip_range(
    _: AuthenticatedRequest,
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<IpRange>, AppError> {
    fetch_ip_details(&state, &ip).map(|f| Json(f))
}

async fn get_ip_range_v2(
    _: AuthenticatedRequest,
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<IpRangeV2>, AppError> {
    fetch_ip_details_v2(&state, &ip).map(|f| Json(f))
}

async fn health_route() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_bulk_event_payload() {
        // Test bulk event structure from mobile team
        let json_str = r#"{
            "event_data": {
                "city": "Mumbai",
                "country": "India",
                "ip_addr": "2402:3a80:16ec:dd61:0:52:b67b:4d01",
                "rows": [
                    {
                        "event_data": {
                            "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
                            "event": "video_viewed",
                            "device": "app",
                            "duration": 120
                        }
                    },
                    {
                        "event_data": {
                            "canister_id": "xxxx-7qaaa-aaaas-qbg3q-cai",
                            "event": "video_liked",
                            "device": "app"
                        }
                    }
                ]
            }
        }"#;

        let payload: Result<EventPayload, _> = serde_json::from_str(json_str);
        assert!(payload.is_ok(), "Failed to deserialize bulk event payload");

        if let Ok(EventPayload::Bulk(bulk)) = payload {
            assert_eq!(bulk.rows.len(), 2);
            assert_eq!(
                bulk.common_fields.get("country").and_then(|v| v.as_str()),
                Some("India")
            );
            assert_eq!(
                bulk.common_fields.get("city").and_then(|v| v.as_str()),
                Some("Mumbai")
            );
        } else {
            panic!("Expected bulk event payload");
        }
    }

    #[test]
    fn test_deserialize_array_event_payload() {
        // Test array of events
        let json_str = r#"[
            {
                "event": "page_view",
                "user_id": "123"
            },
            {
                "event": "click",
                "user_id": "456"
            }
        ]"#;

        let payload: Result<EventPayload, _> = serde_json::from_str(json_str);
        assert!(payload.is_ok(), "Failed to deserialize array event payload");

        if let Ok(EventPayload::Array(events)) = payload {
            assert_eq!(events.len(), 2);
        } else {
            panic!("Expected array event payload");
        }
    }

    #[test]
    fn test_deserialize_single_event_payload() {
        // Test single event object
        let json_str = r#"{
            "event": "page_view",
            "user_id": "123",
            "timestamp": "2024-01-01T00:00:00Z"
        }"#;

        let payload: Result<EventPayload, _> = serde_json::from_str(json_str);
        assert!(
            payload.is_ok(),
            "Failed to deserialize single event payload"
        );

        if let Ok(EventPayload::Single(event)) = payload {
            assert_eq!(
                event.get("event").and_then(|v| v.as_str()),
                Some("page_view")
            );
        } else {
            panic!("Expected single event payload");
        }
    }

    #[test]
    fn test_bulk_event_with_empty_common_fields() {
        // Test bulk event with minimal common fields
        let json_str = r#"{
            "event_data": {
                "rows": [
                    {
                        "event_data": {
                            "event": "test_event"
                        }
                    }
                ]
            }
        }"#;

        let payload: Result<EventPayload, _> = serde_json::from_str(json_str);
        assert!(
            payload.is_ok(),
            "Failed to deserialize bulk event with empty common fields"
        );

        if let Ok(EventPayload::Bulk(bulk)) = payload {
            assert_eq!(bulk.rows.len(), 1);
        } else {
            panic!("Expected bulk event payload");
        }
    }

    #[test]
    fn test_invalid_json_fails() {
        // Test that invalid JSON fails to deserialize
        let json_str = r#"{ invalid json }"#;

        let payload: Result<EventPayload, _> = serde_json::from_str(json_str);
        assert!(payload.is_err(), "Should fail to deserialize invalid JSON");
    }

    #[test]
    fn test_bulk_event_fields_merging() {
        // Test that we can properly extract fields for merging
        let json_str = r#"{
  "ip_addr": "2409:40e3:20cc:bdbc:8000::",
  "city": "patna",
  "rows": [
    {
      "event_data": {
        "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
        "custom_device_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "device": "app",
        "distinct_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "event": "video_impression",
        "feature_name": "feed",
        "game_type": "smiley",
        "is_creator": false,
        "is_game_enabled": true,
        "is_logged_in": false,
        "is_nsfw": false,
        "like_count": 0,
        "publisher_user_id": "nzlex-doomk-jojhy-vaahf-tpr5e-22lig-e2uvd-hho5a-goapc-ozxv2-eqe",
        "share_count": 0,
        "token_type": "yral",
        "type": "com.yral.shared.analytics.events.VideoImpressionEventData",
        "user_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "video_id": "000b249d0cf9bff6fa10907edca6fa74",
        "view_count": 9928,
        "wallet_balance": 25.0
      },
      "timestamp": "2025-11-04T13:56:59.164+00:00"
    },
    {
      "event_data": {
        "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
        "custom_device_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "device": "app",
        "distinct_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "event": "video_started",
        "feature_name": "auth",
        "game_type": "smiley",
        "is_creator": false,
        "is_game_enabled": true,
        "is_logged_in": false,
        "is_nsfw": false,
        "like_count": 0,
        "publisher_user_id": "nzlex-doomk-jojhy-vaahf-tpr5e-22lig-e2uvd-hho5a-goapc-ozxv2-eqe",
        "share_count": 0,
        "token_type": "yral",
        "type": "com.yral.shared.analytics.events.VideoStartedEventData",
        "user_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "video_id": "000b249d0cf9bff6fa10907edca6fa74",
        "view_count": 9928,
        "wallet_balance": 25.0
      },
      "timestamp": "2025-11-04T13:56:59.166+00:00"
    },
    {
      "event_data": {
        "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
        "custom_device_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "device": "app",
        "distinct_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "event": "game_tutorial_shown",
        "feature_name": "feed",
        "game_type": "smiley",
        "is_creator": false,
        "is_logged_in": false,
        "is_nsfw": false,
        "like_count": 0,
        "publisher_user_id": "nzlex-doomk-jojhy-vaahf-tpr5e-22lig-e2uvd-hho5a-goapc-ozxv2-eqe",
        "share_count": 0,
        "token_type": "yral",
        "type": "com.yral.shared.analytics.events.GameTutorialShownEventData",
        "user_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "video_id": "000b249d0cf9bff6fa10907edca6fa74",
        "view_count": 9928,
        "wallet_balance": 25.0
      },
      "timestamp": "2025-11-04T13:56:59.169+00:00"
    },
    {
      "event_data": {
        "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
        "custom_device_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "device": "app",
        "distinct_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "event": "video_impression",
        "feature_name": "feed",
        "game_type": "smiley",
        "is_creator": false,
        "is_game_enabled": true,
        "is_logged_in": false,
        "is_nsfw": false,
        "like_count": 0,
        "publisher_user_id": "ssnde-fhbim-lop65-dyxym-zi2io-l63xv-5b4k6-jqhip-spivo-myzjo-tqe",
        "share_count": 0,
        "token_type": "yral",
        "type": "com.yral.shared.analytics.events.VideoImpressionEventData",
        "user_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "video_id": "0002eba08e48e90bcf8c3b2b62dc038f",
        "view_count": 1426,
        "wallet_balance": 25.0
      },
      "timestamp": "2025-11-04T13:56:59.170+00:00"
    },
    {
      "event_data": {
        "canister_id": "ivkka-7qaaa-aaaas-qbg3q-cai",
        "custom_device_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "device": "app",
        "distinct_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "event": "video_impression",
        "feature_name": "feed",
        "game_type": "smiley",
        "is_creator": false,
        "is_game_enabled": true,
        "is_logged_in": false,
        "is_nsfw": false,
        "like_count": 0,
        "publisher_user_id": "yly54-lozee-55gak-t5l3h-jgpyd-bf55c-swe7r-p2w3w-w4aqx-jfzjn-gae",
        "share_count": 0,
        "token_type": "yral",
        "type": "com.yral.shared.analytics.events.VideoImpressionEventData",
        "user_id": "c724g-fanbu-s4a5s-t3frr-xdgtf-ntg4w-7qne3-mdh2u-id7b7-4xy63-oae",
        "video_id": "0008fc716b00ab7e33f80aa163ca50bc",
        "view_count": 12147,
        "wallet_balance": 25.0
      },
      "timestamp": "2025-11-04T13:56:59.173+00:00"
    }
  ]
}

        "#;

        let payload: EventPayload = serde_json::from_str(json_str).unwrap();

        if let EventPayload::Bulk(bulk) = payload {
            // Verify common fields
            assert!(bulk.common_fields.contains_key("city"));
            assert!(bulk.common_fields.contains_key("country"));
            assert!(bulk.common_fields.contains_key("ip_addr"));

            // Verify row fields
            let row = &bulk.rows[0];
            assert!(row.event_data.fields.contains_key("event"));
            assert!(row.event_data.fields.contains_key("canister_id"));

            // Simulate merging
            let mut merged = bulk.common_fields.clone();
            merged.extend(row.event_data.fields.clone());

            assert_eq!(merged.get("city").and_then(|v| v.as_str()), Some("Mumbai"));
            assert_eq!(
                merged.get("event").and_then(|v| v.as_str()),
                Some("video_started")
            );
        } else {
            panic!("Expected bulk event payload");
        }
    }
}
