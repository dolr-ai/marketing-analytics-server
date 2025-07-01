use base64::decode;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

// Google Cloud Clients
use google_cloud_bigquery::client::{
    Client as BigqueryClient, ClientConfig as BigqueryClientConfig,
};
use google_cloud_pubsub::client::{
    google_cloud_auth::credentials::CredentialsFile, Client as PubsubClient,
    ClientConfig as PubsubClientConfig,
};

#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub project_id: String,
    // Add other application-specific configurations here
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let conf = Config::builder()
            .add_source(File::with_name("config.toml").required(false))
            .add_source(File::with_name(".env").required(false))
            .add_source(Environment::default())
            .build()?;

        conf.try_deserialize()
    }
}

/// Decodes a base64 encoded service account key and parses it into a CredentialsFile.
/// This is a generic function that can be used for any Google Cloud client.
fn parse_credentials_from_base64(encoded_sa_key: &str) -> Result<CredentialsFile, String> {
    let decoded_sa_key = decode(encoded_sa_key)
        .map_err(|e| format!("Failed to decode base64 service account key: {}", e))?;

    let decoded_sa_key_str = String::from_utf8(decoded_sa_key)
        .map_err(|e| format!("Decoded base64 is not valid UTF-8: {}", e))?;

    let creds_file: CredentialsFile = serde_json::from_str(&decoded_sa_key_str).map_err(|e| {
        format!(
            "Provided service account key is not valid JSON for CredentialsFile: {}",
            e
        )
    })?;

    Ok(creds_file)
}

/// Initializes and returns a Google Cloud Pub/Sub client using explicitly provided credentials.
pub async fn get_pubsub_client(
    encoded_sa_key: &str,
) -> Result<PubsubClient, Box<dyn std::error::Error>> {
    let credentials_file = parse_credentials_from_base64(encoded_sa_key)
        .map_err(|e| format!("Failed to parse Pub/Sub credentials: {}", e))?;

    let config = PubsubClientConfig::default()
        .with_credentials(credentials_file)
        .await?; // Await is needed for with_credentials

    let client = PubsubClient::new(config).await?;
    Ok(client)
}

/// Initializes and returns a Google Cloud BigQuery client using explicitly provided credentials.
pub async fn get_bigquery_client(
    encoded_sa_key: &str,
) -> Result<BigqueryClient, Box<dyn std::error::Error>> {
    let credentials_file = parse_credentials_from_base64(encoded_sa_key)
        .map_err(|e| format!("Failed to parse BigQuery credentials: {}", e))?;

    let (config, _) = BigqueryClientConfig::new_with_credentials(credentials_file).await?; // Await is needed for with_credentials

    let client = BigqueryClient::new(config).await?;
    Ok(client)
}
