use infrastructure::repository::mixpanel_repository::MixpanelRepository;

pub mod adapters;
pub mod application;
pub mod config;
use google_cloud_bigquery::client::{Client, ClientConfig};
pub mod app_config;
pub mod ip_config;
pub mod consts;
pub mod domain;
pub mod infrastructure;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
        .init();
    let env_config = crate::config::Config::from_env()?;

    let _ = crate::app_config::AppConfig::load();

    let bigquery_client: Client = init_bigquery_client().await;

    let ip_client = crate::ip_config::IpConfig::load(&env_config.ip_db_path)
        .map_err(|f| tracing::error!("Failed to load IP config: {}", f)).ok();

    let config = adapters::http::HttpServerConfig {
        port: &env_config.server_port.clone(),
    };

    let mixpanel_repository = MixpanelRepository::new(env_config.mixpanel_project_token.clone());

    let analytics_service = application::services::mixpanel_analytics_service::MixpanelService::new(
        mixpanel_repository,
    );

    let http_server =
        adapters::http::HttpServer::new(config, env_config, analytics_service, bigquery_client, ip_client)
            .await
            .expect("Failed to create HTTP server");
    http_server.run().await
}

pub async fn init_bigquery_client() -> Client {
    let (config, _) = ClientConfig::new_with_auth()
        .await
        .map_err(|f| format!("Failed to create BigQuery client: {}", f))
        .unwrap();
    Client::new(config).await.unwrap()
}
