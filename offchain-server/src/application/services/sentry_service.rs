use serde::Serialize;
use std::env;

#[derive(Debug, Serialize)]
struct GoogleChatMessage {
    text: String,
}

pub struct SentryService;

impl SentryService {
    pub fn new() -> Self {
        Self
    }

    pub async fn process_webhook_event(
        &self,
        event: &crate::adapters::sentry_webhook::SentryEvent,
    ) -> Result<(), anyhow::Error> {
        let web_url = event.web_url.as_deref().unwrap_or("N/A");
        let title = event.title.as_deref().unwrap_or("N/A");
        let user_id = event
            .user
            .as_ref()
            .and_then(|user| user.id.as_ref())
            .map(|id| id.as_str())
            .unwrap_or("N/A");
        let level = event.level.as_deref().unwrap_or("unknown");
        let platform = event.platform.as_deref().unwrap_or("unknown");
        let project = event
            .project
            .map(|p| p.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let release = event.release.as_deref().unwrap_or("unknown");

        // Extract environment from tags
        let environment = event
            .tags
            .as_ref()
            .and_then(|tags| {
                tags.iter()
                    .find(|tag| tag[0] == "environment")
                    .map(|tag| tag[1].as_str())
            })
            .unwrap_or("unknown");

        // Send to Google Chat if webhook URL is configured
        if let Ok(webhook_url) = env::var("SENTRY_GOOGLE_CHAT_WEBHOOK_URL") {
            self.send_google_chat_notification(
                &webhook_url,
                title,
                level,
                platform,
                environment,
                &project,
                release,
                user_id,
                web_url,
            )
            .await?;
        } else {
            tracing::debug!(
                "SENTRY_GOOGLE_CHAT_WEBHOOK_URL not configured, skipping Google Chat notification"
            );
        }

        Ok(())
    }

    async fn send_google_chat_notification(
        &self,
        webhook_url: &str,
        title: &str,
        level: &str,
        platform: &str,
        environment: &str,
        project: &str,
        release: &str,
        user_id: &str,
        web_url: &str,
    ) -> Result<(), anyhow::Error> {
        let severity_emoji = match level {
            "error" => "🔴",
            "warning" => "🟡",
            "info" => "🔵",
            "debug" => "⚪",
            "fatal" => "💥",
            _ => "⚠️",
        };

        let message = GoogleChatMessage {
            text: format!(
                "{} *Sentry Alert*\n\n*Title:* {}\n*Level:* {}\n*Platform:* {}\n*Environment:* {}\n*Project:* {}\n*Release:* {}\n*User ID:* {}\n*URL:* {}",
                severity_emoji, title, level, platform, environment, project, release, user_id, web_url
            ),
        };

        let client = reqwest::Client::new();
        match client.post(webhook_url).json(&message).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    tracing::error!(
                        "Failed to send message to Google Chat: {}",
                        response.status()
                    );
                }
            }
            Err(e) => {
                tracing::error!("Error sending message to Google Chat: {}", e);
                return Err(anyhow::anyhow!("Google Chat notification failed: {}", e));
            }
        }

        Ok(())
    }
}
