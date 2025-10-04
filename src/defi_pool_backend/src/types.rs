// Import Candid serialization for Internet Computer (ICP) interfaces
use candid::CandidType;

// Import Serde for JSON (or other formats) serialization/deserialization
use serde::{Deserialize, Serialize};

// Candid arbitrary-precision unsigned integer type
use candid::Nat;

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

/// Represents a request to borrow stablecoins from the pool
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BorrowRequest {
    /// Amount of stablecoin the user wants to borrow
    pub amount: Nat,
}

/// Request payload for Risk Engine proxy
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskRequest {
    pub volatility: Nat,
    pub collateral: Nat,
    pub borrowed: Nat,
    pub deposits: Nat,
    pub credit_score: Nat,
}

/// Response payload from Risk Engine proxy
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskResponse {
    pub risk_score: u8, // 0 = safe, 1 = high_risk
    pub advice: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct StableBalanceEntry {
    pub key: String,
    pub value: Nat,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Default)]
pub struct StableToken {
    pub total_supply: Nat,
    pub balances: Vec<StableBalanceEntry>,
}

