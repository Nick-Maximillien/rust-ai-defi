// src/defi_pool_backend/lib.rs

use ic_cdk_macros::{init, query, update};
use ic_cdk::api::debug_print;
use candid::Principal;
use num_traits::cast::ToPrimitive;

use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use candid::{CandidType, Nat, Deserialize};
use serde::Serialize;
use num_bigint::BigUint;

mod types;
use types::{UserAccount, BorrowRequest, RiskRequest, RiskResponse};

/// Represents a user's stablecoin balance for serialization and JSON.
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct StableBalanceEntry {
    pub key: String,
    pub value: Nat,
}

/// Aggregate stablecoin info for all users.
#[derive(CandidType, Serialize, Deserialize, Clone, Default)]
pub struct StableToken {
    pub total_supply: Nat,
    pub balances: Vec<StableBalanceEntry>,
}

/// Core DeFi pool state containing user accounts, balances, and usernames.
#[derive(Default)]
pub struct DeFiPool {
    pub users: HashMap<String, UserAccount>,
    pub stablecoin_balances: HashMap<String, Nat>,
    pub usernames: HashMap<String, String>,
}

/// Global, thread-safe pool state.
static POOL: Lazy<Mutex<DeFiPool>> = Lazy::new(|| Mutex::new(DeFiPool::default()));
/// Principal of the AI risk canister for async risk evaluation.
static AI_SERVICE_PROXY_PRINCIPAL: Lazy<Mutex<Option<Principal>>> = Lazy::new(|| Mutex::new(None));

/// Canister initialization
#[init]
fn init() {
    let mut pool = POOL.lock().unwrap();
    pool.users = HashMap::new();
    pool.stablecoin_balances = HashMap::new();
    pool.usernames = HashMap::new();
}

/// Register a new user (principal + username).
#[update]
fn signup(user: String, username: String) -> bool {
    debug_print(&format!("Signup called with user='{}', username='{}'", user, username));

    let mut pool = POOL.lock().unwrap();
    if pool.users.contains_key(&user) {
        debug_print(&format!("Signup failed: user '{}' already exists", user));
        return false;
    }

    let mut account = UserAccount::default();
    account.credit_score = Nat::from(700u64);

    pool.users.insert(user.clone(), account);
    pool.usernames.insert(user.clone(), username.clone());

    debug_print(&format!("Signup succeeded: {} -> {}", user, username));
    debug_print(&format!("Current users: {:?}", pool.usernames.keys()));

    true
}

// List all registered users
#[query]
fn list_users() -> Vec<String> {
    let pool = POOL.lock().unwrap();
    pool.users.keys().cloned().collect()
}


/// Get stored username for a principal
#[query]
fn get_username(user: String) -> Option<String> {
    let pool = POOL.lock().unwrap();
    pool.usernames.get(&user).cloned()
}

/// Set the AI service canister principal
#[update]
fn set_ai_proxy(principal: Principal) -> bool {
    let mut p = AI_SERVICE_PROXY_PRINCIPAL.lock().unwrap();
    *p = Some(principal);
    true
}

/// Minimum collateral required (1.5x borrowed)
fn required_collateral(borrowed: &BigUint) -> BigUint {
    (borrowed * 3u32) / 2u32
}

/// Compute total stablecoin supply
fn compute_total_supply(pool: &DeFiPool) -> Nat {
    let total = pool
        .stablecoin_balances
        .values()
        .fold(BigUint::from(0u32), |acc, n| acc + &n.0);
    Nat::from(total)
}

/// Perform async AI risk check; updates account.risk_advice
async fn risk_check(account: &mut UserAccount) -> Option<RiskResponse> {
    let principal = {
        let guard = AI_SERVICE_PROXY_PRINCIPAL.lock().unwrap();
        guard.clone()?
    };

    // Compute simple volatility metric
    let deposits_f64 = account.deposited.0.to_f64().unwrap_or(0.0);
    let borrowed_f64 = account.borrowed.0.to_f64().unwrap_or(0.0);
    let mut volatility = if deposits_f64 > 0.0 { borrowed_f64 / deposits_f64 } else { 0.01 };
    volatility = volatility.clamp(0.01, 0.5);
    let scaled_volatility = (volatility * 1000.0).round() as u64;

    let request = RiskRequest {
        collateral: Nat::from(account.collateral.0.clone()),
        borrowed: Nat::from(account.borrowed.0.clone()),
        deposits: Nat::from(account.deposited.0.clone()),
        volatility: Nat::from(scaled_volatility),
        credit_score: Nat::from(account.deposited.0.clone()),
    };

    debug_print(&format!("Calling AI canister {} with request: {:?}", principal, request));

    let result: Result<(RiskResponse,), _> =
        ic_cdk::call(principal, "risk", (request,)).await;

    if let Ok((resp,)) = result {
        debug_print(&format!("AI canister response: {:?}", resp));
        account.risk_advice = Some(resp.advice.clone());
        Some(resp)
    } else {
        debug_print(&format!("AI canister call failed: {:?}", result.err()));
        account.risk_advice = Some("AI service unavailable".to_string());
        None
    }
}

/// Deposit stablecoins (updates both account and pool balances)
#[update]
fn deposit(user: String, amount: Nat) -> bool {
    let mut pool = POOL.lock().unwrap();
    let account = pool.users.entry(user.clone()).or_default();
    account.deposited = Nat::from(&account.deposited.0 + &amount.0);

    let bal = pool.stablecoin_balances.entry(user.clone()).or_insert(Nat::from(0u64));
    *bal = Nat::from(&bal.0 + &amount.0);

    true
}

/// Deposit collateral with AI risk check
#[update]
async fn deposit_collateral(user: String, amount: Nat) -> bool {
    // Lock pool and get mutable reference to account
    let mut pool = POOL.lock().unwrap();
    let account = match pool.users.get_mut(&user) {
        Some(acc) => acc,
        None => {
            debug_print(&format!("Deposit collateral failed: user '{}' not found", user));
            return false;
        }
    };

    // Tentatively increase collateral
    let potential_collateral = Nat::from(&account.collateral.0 + &amount.0);

    // Ensure minimum collateral after deposit
    let required = required_collateral(&account.borrowed.0);
    if potential_collateral.0 < required {
        account.risk_advice = Some("Collateral insufficient for current borrowed amount".to_string());
        return false;
    }

    account.collateral = potential_collateral.clone();

    // Release lock before awaiting async AI check
    drop(pool);

    // Perform AI risk check
    if let Some(mut pool) = POOL.lock().ok() {
        let account = pool.users.get_mut(&user).unwrap(); // safe to unwrap
        if let Some(risk) = risk_check(account).await {
            if risk.risk_score > 0 {
                // Revert collateral increase on high risk
                account.collateral = Nat::from(&account.collateral.0 - &amount.0);
                return false;
            }
        }
    }

    true
}

/// Borrow stablecoins with AI risk check
#[update]
async fn borrow(user: String, request: BorrowRequest) -> bool {
    // Lock pool and get mutable reference to account
    let mut pool = POOL.lock().unwrap();
    let account = match pool.users.get_mut(&user) {
        Some(acc) => acc,
        None => {
            debug_print(&format!("Borrow failed: user '{}' not found", user));
            return false;
        }
    };

    // Compute potential new borrowed amount
    let potential_borrowed = Nat::from(&account.borrowed.0 + &request.amount.0);

    // Ensure user has enough collateral before borrowing
    let required = required_collateral(&potential_borrowed.0);
    if account.collateral.0 < required {
        account.risk_advice = Some("Insufficient collateral to borrow requested amount".to_string());
        return false;
    }

    // Tentatively increase borrowed amount
    account.borrowed = potential_borrowed.clone();

    // Release lock before awaiting async AI check
    drop(pool);

    // Perform AI risk check
    if let Some(mut pool) = POOL.lock().ok() {
        let account = pool.users.get_mut(&user).unwrap(); // safe to unwrap
        if let Some(risk) = risk_check(account).await {
            if risk.risk_score > 0 {
                // Revert borrowed amount on high risk
                account.borrowed = Nat::from(&account.borrowed.0 - &request.amount.0);
                return false;
            }
        }
    }

    // Lock pool again to update stablecoin balance
    let mut pool = POOL.lock().unwrap();
    let bal = pool.stablecoin_balances.entry(user.clone()).or_insert(Nat::from(0u64));
    *bal = Nat::from(&bal.0 + &request.amount.0);

    true
}



/// Repay borrowed stablecoins
#[update]
fn repay(user: String, amount: Nat) -> bool {
    let mut pool = POOL.lock().unwrap();
    if let Some(account) = pool.users.get_mut(&user) {
        if account.borrowed.0 < amount.0 {
            return false;
        }
        account.borrowed = Nat::from(&account.borrowed.0 - &amount.0);
        let bal = pool.stablecoin_balances.entry(user.clone()).or_insert(Nat::from(0u64));
        if bal.0 < amount.0 {
            return false;
        }
        *bal = Nat::from(&bal.0 - &amount.0);
        true
    } else {
        false
    }
}

/// Withdraw collateral while respecting minimum required collateral
#[update]
fn withdraw_collateral(user: String, amount: Nat) -> bool {
    let mut pool = POOL.lock().unwrap();

    if let Some(account) = pool.users.get_mut(&user) {
        // Ensure user has enough collateral to withdraw
        if account.collateral.0 < amount.0 {
            account.risk_advice = Some("Insufficient collateral to withdraw".to_string());
            return false;
        }

        // Check that after withdrawal, minimum collateral is maintained
        let remaining_collateral = &account.collateral.0 - &amount.0;
        let required = required_collateral(&account.borrowed.0);
        if remaining_collateral < required {
            account.risk_advice = Some("Cannot withdraw: would breach minimum collateral".to_string());
            return false;
        }

        account.collateral = Nat::from(remaining_collateral);
        account.risk_advice = Some("Collateral withdrawn successfully".to_string());
        true
    } else {
        false
    }
}


/// Query stablecoin total supply and all balances
/// Query stablecoin total supply AND all balances
#[query]
fn get_stable_token() -> StableToken {
    let pool = POOL.lock().unwrap();

    let balances: Vec<StableBalanceEntry> = pool
        .stablecoin_balances
        .iter()
        .map(|(k, v)| StableBalanceEntry {
            key: k.clone(),
            value: v.clone(),
        })
        .collect();

    let total_supply = compute_total_supply(&pool);

    StableToken {
        total_supply,
        balances,
    }
}


/// Query user account details
#[query]
fn get_user_account(user: String) -> Option<UserAccount> {
    let pool = POOL.lock().unwrap();
    if let Some(mut account) = pool.users.get(&user).cloned() {
        account.username = pool.usernames.get(&user).cloned();
        Some(account)
    } else {
        None
    }
}


/// Query balance
#[query]
fn get_balance(user: String) -> Nat {
    let pool = POOL.lock().unwrap();
    pool.stablecoin_balances.get(&user).cloned().unwrap_or(Nat::from(0u64))
}
/// Query total stablecoin supply and all balances
#[query]
fn get_total_supply() -> Nat {
    let pool = POOL.lock().unwrap();
    compute_total_supply(&pool)
}
