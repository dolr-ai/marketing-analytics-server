use crate::{adapters::app_state::AppState, domain::errors::AppError, ip_config::IpRange};
use candid::{CandidType, Decode, Encode, Nat};
use ic_agent::{export::Principal, Agent};
use reqwest::Client;
use serde::*;
use woothee::parser::Parser;
use yral_canisters_client::{
    ic::{USER_INFO_SERVICE_ID, USER_POST_SERVICE_ID},
    individual_user_template::{IndividualUserTemplate, Result6},
    user_post_service::{Result3, UserPostService},
};

#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct Icrc1Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(CandidType, Deserialize)]
pub enum Result1 {
    Ok(Vec<PostDetailsForFrontend>),
    Err(GetPostsOfUserProfileError),
}

#[derive(CandidType, Deserialize)]
pub enum GetPostsOfUserProfileError {
    ReachedEndOfItemsList,
    InvalidBoundsPassed,
    ExceededMaxNumberOfItemsAllowedInOneRequest,
}

#[derive(CandidType, Deserialize)]
pub struct PostDetailsForFrontend {
    pub id: u64,
}

fn get_agent() -> Agent {
    let url = "https://ic0.app";
    Agent::builder().with_url(url).build().unwrap()
}

pub async fn btc_balance_of(owner: Principal) -> Result<u64, AppError> {
    let agent = get_agent();
    let args = Encode!(&Icrc1Account {
        owner,
        subaccount: None,
    })
    .unwrap();
    let bytes = agent
        .query(
            &Principal::from_text(crate::consts::BTC_LEDGER_CANISTER).unwrap(),
            "icrc1_balance_of",
        )
        .with_arg(args)
        .call()
        .await?;
    let bal = Decode!(&bytes, Nat).map(|nat_bal| nat_bal.0.clone().to_string().parse::<u64>())?;
    Ok(bal?)
}

pub async fn is_creator(
    user_principal: Principal,
    user_canister: Principal,
) -> Result<bool, AppError> {
    let agent = get_agent();

    match user_canister {
        USER_INFO_SERVICE_ID => {
            let user_post_service = UserPostService(USER_POST_SERVICE_ID, &agent);
            let posts_result = user_post_service
                .get_posts_of_this_user_profile_with_pagination(user_principal, 0, 1)
                .await?;

            match posts_result {
                Result3::Ok(posts) => Ok(!posts.is_empty()),
                Result3::Err(_e) => Ok(false),
            }
        }
        _ => {
            let individual_user_service = IndividualUserTemplate(user_canister, &agent);
            let post_results = individual_user_service
                .get_posts_of_this_user_profile_with_pagination_cursor(064, 10u64)
                .await?;
            match post_results {
                Result6::Ok(posts) => Ok(!posts.is_empty()),
                Result6::Err(_e) => Ok(false),
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SatsBalance {
    balance: Vec<f32>,
}

pub async fn sats_balance_of(user: Principal) -> Result<f64, AppError> {
    let url = format!("{}/{}", crate::consts::SATS_BALANCE_URL, user.to_text());
    Ok((*Client::new()
        .get(url)
        .send()
        .await?
        .json::<SatsBalance>()
        .await?
        .balance
        .first()
        .ok_or(AppError::InvalidData("Missing SATs balance".into()))?)
    .into())
}

pub fn classify_device(user_agent: &str) -> &'static str {
    let parser = Parser::new();
    parser
        .parse(user_agent)
        .map(|result| match result.category.to_lowercase().as_str() {
            "smartphone" | "mobilephone" | "tablet" => "mweb",
            "pc" => "web",
            _ => "other",
        })
        .unwrap_or("other")
}

pub fn fetch_ip_details(state: &AppState, ip: &str) -> Result<IpRange, AppError> {
    state
        .ip_client
        .as_ref()
        .ok_or(AppError::IpConfigError("IP config not loaded".into()))?
        .look_up(&ip)
        .ok_or(AppError::InvalidData(format!("IP not found: {}", ip)))
        .map_err(|e| AppError::IpConfigError(format!("Failed to look up IP: {}", e)))
}
