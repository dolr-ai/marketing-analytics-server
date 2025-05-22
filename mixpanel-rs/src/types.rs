#[derive(Debug, Clone)]
pub struct Config {
    pub debug: bool,
    pub host: String,
    pub protocol: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug: false,
            host: "api.mixpanel.com".to_string(),
            protocol: "https".to_string(),
        }
    }
}
