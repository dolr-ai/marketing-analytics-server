use std::{
    env,
    fs::OpenOptions,
    io::BufWriter,
};
use serde::Deserialize;
use config::{Config, ConfigError, Environment, File};
use serde_json::Value;

#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub google_sa_key: String,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let sa_key_raw = env::var("GOOGLE_SA_KEY").expect("GOOGLE_SA_KEY is missing");

        // Validate and parse JSON (avoids trailing character issues)
        let parsed_json: Value = serde_json::from_str(&sa_key_raw)
            .expect("GOOGLE_SA_KEY is not valid JSON");

        // Write the validated, pretty-printed JSON to file
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("google_service_account.json")
            .expect("failed to create json file");

        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &parsed_json)
            .expect("failed to write json");

        env::set_var("GOOGLE_APPLICATION_CREDENTIALS", "google_service_account.json");

        let conf = Config::builder()
            .add_source(File::with_name("config.toml").required(false))
            .add_source(File::with_name(".env").required(false))
            .add_source(Environment::default())
            .build()?;

        conf.try_deserialize()
    }
}
