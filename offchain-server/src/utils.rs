use candid::{CandidType, Decode, Encode, Nat};
use ic_agent::{Agent, export::Principal};
use serde::*;

#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct Icrc1Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

pub async fn btc_balance_of(owner: Principal) -> Result<u64, String> {
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
        .await
        .map_err(|f| {
            format!(
                "failed to fetch balance for principal: {:?} with error: {:?}",
                owner.to_text(),
                f
            )
        })?;
    let bal = Decode!(&bytes, Nat)
        .map_err(|f| format!("Failed to parse response {}", f))
        .map(|nat_bal| nat_bal.0.clone().to_string().parse::<u64>())?;
    bal.map_err(|f| format!("Parse error {:?}", f))
}
