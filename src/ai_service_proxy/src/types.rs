use candid::CandidType;
use candid::Nat;
use serde::{Serialize, Deserialize};

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct RiskRequest {
    pub volatility: Nat,
    pub collateral: Nat,
    pub borrowed: Nat,
    pub deposits: Nat,
    pub credit_score: Nat,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct RiskResponse {
    pub risk_score: u8, // 0 = safe, 1 = high risk
    pub advice: String,
}
