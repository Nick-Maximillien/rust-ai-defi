use ic_cdk_macros::{init, query, update};
use candid::{CandidType, Nat, Principal, Deserialize};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// User allowance structure
#[derive(Clone, CandidType, Deserialize, Serialize)]
pub struct Allowance {
    pub spender: Principal,
    pub amount: Nat,
}

/// Core DIP-20 state
#[derive(Default)]
pub struct DIP20Token {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Nat,
    pub balances: HashMap<Principal, Nat>,
    pub allowances: HashMap<(Principal, Principal), Nat>, // (owner, spender) -> allowance
}

static TOKEN: Lazy<Mutex<DIP20Token>> = Lazy::new(|| Mutex::new(DIP20Token::default()));
static POOL_CANISTER: Lazy<Mutex<Option<Principal>>> = Lazy::new(|| Mutex::new(None));

#[init]
fn init() {
    let mut token = TOKEN.lock().unwrap();
    token.name = "ICP Token".to_string();      
    token.symbol = "ICP".to_string();     
    token.decimals = 8;
    token.total_supply = Nat::from(0u64);
}

#[update]
fn set_pool_canister(pool: Principal) -> bool {
    let mut guard = POOL_CANISTER.lock().unwrap();
    *guard = Some(pool);
    true
}

#[query]
fn name() -> String {
    let token = TOKEN.lock().unwrap();
    token.name.clone()
}

#[query]
fn symbol() -> String {
    let token = TOKEN.lock().unwrap();
    token.symbol.clone()
}

#[query]
fn decimals() -> u8 {
    let token = TOKEN.lock().unwrap();
    token.decimals
}

#[query]
fn total_supply() -> Nat {
    let token = TOKEN.lock().unwrap();
    token.total_supply.clone()
}

#[query]
fn balanceOf(owner: Principal) -> Nat {
    let token = TOKEN.lock().unwrap();
    token.balances.get(&owner).cloned().unwrap_or(Nat::from(0u64))
}

#[query]
fn allowance(owner: Principal, spender: Principal) -> Nat {
    let token = TOKEN.lock().unwrap();
    token.allowances.get(&(owner, spender)).cloned().unwrap_or(Nat::from(0u64))
}

#[update]
fn approve(spender: Principal, amount: Nat) -> bool {
    let caller = ic_cdk::caller();
    let mut token = TOKEN.lock().unwrap();
    token.allowances.insert((caller, spender), amount);
    true
}

#[update]
fn transfer(to: Principal, amount: Nat) -> bool {
    let caller = ic_cdk::caller();
    let mut token = TOKEN.lock().unwrap();
    let sender_balance = token.balances.get(&caller).cloned().unwrap_or(Nat::from(0u64));
    if sender_balance.0 < amount.0 {
        return false;
    }
    token.balances.insert(caller, Nat::from(&sender_balance.0 - &amount.0));
    let to_balance = token.balances.get(&to).cloned().unwrap_or(Nat::from(0u64));
    token.balances.insert(to, Nat::from(&to_balance.0 + &amount.0));
    true
}

#[update]
fn transferFrom(from: Principal, to: Principal, amount: Nat) -> bool {
    let caller = ic_cdk::caller();
    let mut token = TOKEN.lock().unwrap();
    let allowed = token.allowances.get(&(from, caller)).cloned().unwrap_or(Nat::from(0u64));
    if allowed.0 < amount.0 {
        return false;
    }
    let from_balance = token.balances.get(&from).cloned().unwrap_or(Nat::from(0u64));
    if from_balance.0 < amount.0 {
        return false;
    }
    token.balances.insert(from, Nat::from(&from_balance.0 - &amount.0));
    let to_balance = token.balances.get(&to).cloned().unwrap_or(Nat::from(0u64));
    token.balances.insert(to, Nat::from(&to_balance.0 + &amount.0));
    token.allowances.insert((from, caller), Nat::from(&allowed.0 - &amount.0));
    true
}

#[update]
fn mint(to: Principal, amount: Nat) -> bool {
    // Allow any caller for local testing
    // let pool_guard = POOL_CANISTER.lock().unwrap();
    // let pool_principal = pool_guard.unwrap_or(Principal::anonymous());
    // if caller != pool_principal { return false; }

    let mut token = TOKEN.lock().unwrap();
    let to_balance = token.balances.get(&to).cloned().unwrap_or(Nat::from(0u64));
    token.balances.insert(to, Nat::from(&to_balance.0 + &amount.0));
    token.total_supply = Nat::from(&token.total_supply.0 + &amount.0);
    true
}


