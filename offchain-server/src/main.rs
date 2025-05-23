use infrastructure::repository::mixpanel_repository::MixpanelRepository;

pub mod adapters;
pub mod application;
pub mod config;
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

    let config = adapters::http::HttpServerConfig {
        port: &env_config.server_port.clone(),
    };

    let mixpanel_repository = MixpanelRepository::new(env_config.mixpanel_project_token.clone());

    let analytics_service = application::services::mixpanel_analytics_service::MixpanelService::new(
        mixpanel_repository,
    );

    let http_server = adapters::http::HttpServer::new(config, env_config, analytics_service)
        .await
        .expect("Failed to create HTTP server");
    http_server.run().await
}
