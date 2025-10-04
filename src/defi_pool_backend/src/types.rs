use candid::CandidType;
use candid::Nat;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Represents a user's account in the DeFi pool
#[derive(CandidType, Serialize, Deserialize, Clone, Default)]
pub struct UserAccount {
    pub deposited: Nat,
    pub borrowed: Nat,
    pub collateral: Nat,
    pub credit_score: Nat,
    pub risk_advice: Option<String>,
    pub username: Option<String>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BorrowRequest {
    /// Token identifier, e.g., "ICP", "FAKEBTC"
    pub token: String,
    /// Amount to borrow
    pub amount: Nat,
}

/// Request payload for AI Risk Engine
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskRequest {
    pub volatility: Nat,
    pub collateral: Nat,
    pub borrowed: Nat,
    pub deposits: Nat,
    pub credit_score: Nat,
}

/// Response payload from AI Risk Engine
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskResponse {
    pub risk_score: u8, // 0 = safe, 1 = high risk
    pub advice: String,
}

/// Represents a balance entry for a specific token
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct StableBalanceEntry {
    pub token: String,
    pub value: Nat,
}

/// Aggregated token balances for all users
#[derive(CandidType, Serialize, Deserialize, Clone, Default)]
pub struct StableToken {
    pub total_supply: Nat,
    pub balances: Vec<StableBalanceEntry>,
}

/// Crowdfunding entry for a user's contribution
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct CrowdfundEntry {
    pub user: String,
    pub token: String,
    pub amount: Nat,
}

/// Crowdfunding pool structure
#[derive(Default)]
pub struct CrowdfundingPool {
    pub funds: HashMap<String, Nat>,                     // token -> total
    pub contributors: HashMap<String, HashMap<String, Nat>>, // user -> token -> amount
}
