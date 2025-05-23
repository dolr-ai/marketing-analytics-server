use std::env;

use anyhow::Context;

const SERVER_PORT_KEY: &str = "SERVER_PORT";

const SERVER_ACCESS_TOKEN: &str = "SERVER_ACCESS_TOKEN";

const MIXPANEL_PROJECT_TOKEN: &str = "MIXPANEL_PROJECT_TOKEN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub server_port: String,
    pub server_access_token: String,
    pub mixpanel_project_token: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        dotenv::dotenv().ok();

        let server_port = load_env(SERVER_PORT_KEY).unwrap_or("3000".to_string());

        let server_access_token =
            load_env(SERVER_ACCESS_TOKEN).context("Failed to get server access token")?;

        let mixpanel_project_token =
            load_env(MIXPANEL_PROJECT_TOKEN).context("Failed to get mixpanel project token")?;

        Ok(Config {
            server_port,
            server_access_token,
            mixpanel_project_token,
        })
    }
}

fn load_env(key: &str) -> anyhow::Result<String> {
    env::var(key).with_context(|| format!("failed to load environment variable {}", key))
}
