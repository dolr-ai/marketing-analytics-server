use mixpanel_rs::Mixpanel;
use serde_json::json;

#[cfg(feature = "tracing")]
fn init_tracing() {
    use tracing_subscriber::FmtSubscriber;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

#[tokio::main]
async fn main() {
    #[cfg(feature = "tracing")]
    init_tracing();

    let mixpanel = Mixpanel::init("YOUR_PROJECT_TOKEN", None);

    let device_id_1 = uuid::Uuid::new_v4().to_string();

    let distinct_id = "cp7dg-n36pb-3bcja-caqkm-vcanj-t37c7-p7ptb-h3tls-6srot-2jz7m-6ae";

    let _ =  mixpanel.people.set(distinct_id , json!({
        "$email" : "user@email.com", 
        "$device_id": &device_id_1,
        "$user_id": distinct_id,
    })).await;

    let _ = mixpanel
        .track(
            "example_event-3",
            Some(json!({
                "distinct_id": distinct_id,
                "button": "signup",
                "$device_id": &device_id_1,
                "$user_id": distinct_id,
            })),
        )
        .await;

    let _ = mixpanel
        .track(
            "example_event-4",
            Some(json!({
                "distinct_id": distinct_id,
                "button": "login",
                "$device_id": &device_id_1,
                "$user_id": distinct_id,
            })),
        )
        .await;

    let _ = mixpanel
        .track(
            "example_event-5",
            Some(json!({
                "distinct_id": distinct_id,
                "button": "home",
                "$device_id": &device_id_1,
                "$user_id": distinct_id,
            })),
        )
        .await;
}
