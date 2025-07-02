use std::sync::Arc;

use google_cloud_pubsub::publisher::Publisher;

use crate::{
    application::services, config::Config,
    infrastructure::repository::mixpanel_repository::MixpanelRepository,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub analytics_service:
        Arc<services::mixpanel_analytics_service::MixpanelService<MixpanelRepository>>,
    pub bigquery_client: google_cloud_bigquery::client::Client,
    pub pubsub_client: Arc<google_cloud_pubsub::client::Client>,
    pub ip_client: Option<Arc<crate::ip_config::IpConfig>>,
    pub pubsub_event_publisher: Arc<Publisher>,
}
