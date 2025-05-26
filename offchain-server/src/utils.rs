use candid::{CandidType, Decode, Encode, Nat};
use ic_agent::{Agent, export::Principal};
use reqwest::Client;
use serde::*;

use crate::domain::errors::AppError;

#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct Icrc1Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

pub async fn btc_balance_of(owner: Principal) -> Result<u64, AppError> {
    let url = "https://ic0.app";
    let agent = Agent::builder().with_url(url).build().unwrap();
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
