use std::env;

use anyhow::Context;

const SERVER_PORT_KEY: &str = "SERVER_PORT";

const SERVER_ACCESS_TOKEN: &str = "SERVER_ACCESS_TOKEN";

const MIXPANEL_PROJECT_TOKEN: &str = "MIXPANEL_PROJECT_TOKEN";

const GOOGLE_PUBSUB_KEY: &str = "GOOGLE_PUBSUB_KEY";

const GOOGLE_SA_KEY: &str = "GOOGLE_SA_KEY";

const IP_DB_PATH: &str = "IP_DB_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub server_port: String,
    pub server_access_token: String,
    pub mixpanel_project_token: String,
    pub ip_db_path: String,
    pub bigquery_access_key: String,
    pub pub_sub_access_key: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        dotenv::dotenv().ok();

        let server_port = load_env(SERVER_PORT_KEY).unwrap_or("3000".to_string());

        let server_access_token =
            load_env(SERVER_ACCESS_TOKEN).context("Failed to get server access token")?;

        let mixpanel_project_token =
            load_env(MIXPANEL_PROJECT_TOKEN).context("Failed to get mixpanel project token")?;

        let bigquery_access_key =
            load_env(GOOGLE_SA_KEY).context("Failed to get GOOGLE_SA_KEY project token")?;

        let pub_sub_access_key =
            load_env(GOOGLE_PUBSUB_KEY).context("Failed to get GOOGLE_PUBSUB_KEY project token")?;

        let ip_db_path = load_env(IP_DB_PATH).unwrap_or("ip_db.csv".to_string());

        Ok(Config {
            server_port,
            server_access_token,
            mixpanel_project_token,
            ip_db_path,
            pub_sub_access_key,
            bigquery_access_key,
        })
    }
}

fn load_env(key: &str) -> anyhow::Result<String> {
    env::var(key).with_context(|| format!("failed to load environment variable {}", key))
}
