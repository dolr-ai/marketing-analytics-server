use std::net::SocketAddr;

use anyhow::Context;
use axum::{Json, Router, extract::Path, http::StatusCode, response::IntoResponse, routing::*};
use candid::Principal;
use mixpanel_rs::Mixpanel;
use serde_json::{Value, json};
use tokio::net;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerConfig<'a> {
    pub port: &'a str,
}

pub struct HttpServer {
    router: Router,
    listener: net::TcpListener,
}

impl HttpServer {
    pub async fn new(config: HttpServerConfig<'_>) -> anyhow::Result<Self> {
        let trace_layer =
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request<_>| {
                let uri = request.uri().to_string();
                tracing::info_span!("http_request", method = ?request.method(), uri)
            });

        let router = Router::new()
            .route("/health", get(health_route))
            .nest("/api", api_routes())
            .layer(trace_layer)
            .layer(CorsLayer::permissive());

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

fn api_routes() -> Router {
    Router::new()
        .route("/btc_balance/{principal}", get(fetch_btc_balance))
        .route("/send_event", post(send_event_to_mixpanel))
}

#[derive(serde::Serialize)]
struct Balance {
    balance: f64,
}

async fn fetch_btc_balance(
    Path(principal): Path<Principal>,
) -> Result<Json<Balance>, impl IntoResponse> {
    match crate::utils::btc_balance_of(principal).await {
        Ok(bal) => {
            let balance = bal as f64 / 100_000_000.0;
            Ok(Json(Balance { balance }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e})))),
    }
}

async fn send_event_to_mixpanel(Json(payload): Json<Value>) {
    let mut payload = payload;
    let principal = payload.get("principal").and_then(|f| f.as_str()).map(str::to_owned);
    let event = payload.get("event").map(|f| f.to_string()).unwrap_or("unknown".into());
    if principal.is_some() {
        match crate::utils::btc_balance_of(Principal::from_text(principal.clone().unwrap()).unwrap()).await
        {
            Ok(bal) => {
                payload["btc_balance"] = (bal as f64 / 100_000_000.0).into();
                payload["$user_id"] = principal.unwrap().into();
            }
            Err(_) => {}
        }
    }
    mixpanel_event(&event, payload).await
}

async fn mixpanel_event(event: &str, properties: Value) {
    let mixpanel = Mixpanel::init("28e0a9dbb1f1624ef951b6217180d483", None);
    let _ =  mixpanel.track(event, Some(properties)).await;
}

async fn health_route() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}
