pub mod adapters;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
        .init();
    let config = adapters::http::HttpServerConfig {
        port: "3000".into(),
    };
    let http_server = adapters::http::HttpServer::new(config)
        .await
        .expect("Failed to create HTTP server");
    http_server.run().await.expect("Failed to run HTTP server");
}
