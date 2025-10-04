// src/dip20_token/types.rs

use candid::{CandidType, Nat, Principal};
use serde::{Deserialize, Serialize};

#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct Allowance {
    pub spender: Principal,
    pub amount: Nat,
}
