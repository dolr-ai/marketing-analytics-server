use crate::app_config::{get_bigquery_client, get_pubsub_client};
use infrastructure::repository::mixpanel_repository::MixpanelRepository;

pub mod adapters;
pub mod app_config;
pub mod application;
pub mod config;
pub mod consts;
pub mod domain;
pub mod infrastructure;
pub mod ip_config;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
        .init();
    let env_config = crate::config::Config::from_env()?;

    let _ = crate::app_config::AppConfig::load();

    let bigquery_client = get_bigquery_client(&env_config.bigquery_access_key)
        .await
        .map_err(|f| tracing::error!("Failed to load bigquery client: {}", f))
        .unwrap();

    let pubsub_client = get_pubsub_client(&env_config.pub_sub_access_key)
        .await
        .map_err(|f| tracing::error!("Failed to load pubsub client: {}", f))
        .unwrap();

    let ip_client = crate::ip_config::IpConfig::load(&env_config.ip_db_path)
        .map_err(|f| tracing::error!("Failed to load IP config: {}", f))
        .ok();

    let ip_client = crate::ip_config::IpConfig::load(&env_config.ip_db_path)
        .map_err(|f| tracing::error!("Failed to load IP config: {}", f)).ok();

    let config = adapters::http::HttpServerConfig {
        port: &env_config.server_port.clone(),
    };

    let mixpanel_repository = MixpanelRepository::new(env_config.mixpanel_project_token.clone());

    let analytics_service = application::services::mixpanel_analytics_service::MixpanelService::new(
        mixpanel_repository,
    );

    let http_server = adapters::http::HttpServer::new(
        config,
        env_config,
        analytics_service,
        bigquery_client,
        pubsub_client,
        ip_client,
    )
    .await
    .expect("Failed to create HTTP server");
    http_server.run().await
}
