use ic_cdk_macros::{init, query, update};
use candid::{CandidType, Nat, Principal, Deserialize};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;
use ic_cdk::api::canister_self;
use ic_cdk::call;

mod types;
use types::{UserAccount, BorrowRequest, RiskRequest, RiskResponse, StableBalanceEntry, StableToken, CrowdfundEntry};

/// DIP-20 helper functions
mod dip20 {
    use candid::{Nat, Principal};
    use ic_cdk::call;

    pub async fn transfer(token: Principal, from: Principal, to: Principal, amount: Nat) -> bool {
        let res: Result<(bool,), _> = call(token, "transferFrom", (from, to, amount)).await;
        res.map(|(ok,)| ok).unwrap_or(false)
    }

    pub async fn balance_of(token: Principal, owner: Principal) -> Nat {
        let res: Result<(Nat,), _> = call(token, "balanceOf", (owner,)).await;
        res.map(|(b,)| b).unwrap_or(Nat::from(0u64))
    }

    pub async fn mint(token: Principal, to: Principal, amount: Nat) -> bool {
        let res: Result<(bool,), _> = call(token, "mint", (to, amount)).await;
        res.map(|(ok,)| ok).unwrap_or(false)
    }
}

/// Multi-token collateral entry
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct CollateralEntry {
    pub token: String,
    pub amount: Nat,
}

/// Crowdfunding pool
#[derive(Default)]
pub struct CrowdfundingPool {
    pub funds: HashMap<String, Nat>, 
    pub contributors: HashMap<String, HashMap<String, Nat>>, 
}

/// Core DeFi pool state
#[derive(Default)]
pub struct DeFiPool {
    pub users: HashMap<String, UserAccount>,
    pub stablecoin_balances: HashMap<String, HashMap<String, Nat>>, 
    pub collateral: HashMap<String, HashMap<String, Nat>>,          
    pub usernames: HashMap<String, String>,
    pub supported_tokens: Vec<String>, 
    pub token_canisters: HashMap<String, Principal>, 
    // --- Mint logs
    pub mint_logs: Vec<(String, String, Nat)>, // (user, token, amount)
    pub per_user_mint_logs: HashMap<String, Vec<(String, Nat)>>, // user -> Vec<(token, amount)>
}

/// Global state
static POOL: Lazy<Mutex<DeFiPool>> = Lazy::new(|| Mutex::new(DeFiPool::default()));
static CF_POOL: Lazy<Mutex<CrowdfundingPool>> =
    Lazy::new(|| Mutex::new(CrowdfundingPool::default()));
static AI_SERVICE_PROXY_PRINCIPAL: Lazy<Mutex<Option<Principal>>> =
    Lazy::new(|| Mutex::new(None));

#[update]
fn init_tokens() -> bool {
    let mut pool = POOL.lock().unwrap();
    if pool.supported_tokens.is_empty() {
        pool.supported_tokens = vec!["ICP".to_string(), "FAKEBTC".to_string(), "FAKEETH".to_string()];
        match Principal::from_text("ulvla-h7777-77774-qaacq-cai") {
            Ok(icp_canister) => {
                pool.token_canisters.insert("ICP".to_string(), icp_canister);
            }
            Err(err) => ic_cdk::print(format!("Failed to parse ICP canister ID: {:?}", err)),
        }
        return true;
    }
    false
}

// ---------------- USER MANAGEMENT ----------------

#[update]
fn signup(user: String, username: String) -> bool {
    let mut pool = POOL.lock().unwrap();
    if pool.users.contains_key(&user) {
        return false;
    }

    let mut account = UserAccount::default();
    account.credit_score = Nat::from(700u64);

    pool.users.insert(user.clone(), account);
    pool.usernames.insert(user.clone(), username);
    true
}

#[query]
fn list_users() -> Vec<String> {
    let pool = POOL.lock().unwrap();
    pool.users.keys().cloned().collect()
}

#[query]
fn get_username(user: String) -> Option<String> {
    let pool = POOL.lock().unwrap();
    pool.usernames.get(&user).cloned()
}

#[update]
fn set_ai_proxy(principal: Principal) -> bool {
    let mut p = AI_SERVICE_PROXY_PRINCIPAL.lock().unwrap();
    *p = Some(principal);
    true
}

#[update]
fn add_token(token: String, principal: Principal) -> bool {
    let mut pool = POOL.lock().unwrap();
    if pool.supported_tokens.contains(&token) {
        pool.token_canisters.insert(token.clone(), principal);
        true
    } else {
        false
    }
}

/// Compute total supply
fn compute_total_supply(pool: &DeFiPool) -> Nat {
    let mut total = BigUint::from(0u32);
    for user_balances in pool.stablecoin_balances.values() {
        for bal in user_balances.values() {
            total += &bal.0;
        }
    }
    Nat::from(total)
}

fn aggregate_collateral(account_collateral: &HashMap<String, Nat>) -> f64 {
    account_collateral
        .iter()
        .map(|(token, amt)| {
            let price = match token.as_str() {
                "ICP" => 1.0,
                "FAKEBTC" => 50000.0,
                "FAKEETH" => 3000.0,
                _ => 1.0,
            };
            amt.0.to_f64().unwrap_or(0.0) * price
        })
        .sum()
}

fn aggregate_borrowed(account_borrowed: &HashMap<String, Nat>) -> f64 {
    account_borrowed
        .iter()
        .map(|(token, amt)| {
            let price = match token.as_str() {
                "ICP" => 1.0,
                "FAKEBTC" => 50000.0,
                "FAKEETH" => 3000.0,
                _ => 1.0,
            };
            amt.0.to_f64().unwrap_or(0.0) * price
        })
        .sum()
}

fn aggregate_deposits(account_balances: &HashMap<String, Nat>) -> f64 {
    account_balances
        .iter()
        .map(|(token, amt)| {
            let price = match token.as_str() {
                "ICP" => 1.0,
                "FAKEBTC" => 50000.0,
                "FAKEETH" => 3000.0,
                _ => 1.0,
            };
            amt.0.to_f64().unwrap_or(0.0) * price
        })
        .sum()
}

/// AI risk check
async fn risk_check(
    account: &mut UserAccount,
    coll_usd: f64,
    borrowed_usd: f64,
    deposits_usd: f64,
) -> Option<RiskResponse> {
    let principal = {
        let guard = AI_SERVICE_PROXY_PRINCIPAL.lock().unwrap();
        guard.clone()?
    };

    let volatility = if deposits_usd > 0.0 {
        borrowed_usd / deposits_usd
    } else {
        0.01
    };
    let scaled_vol = (volatility.clamp(0.01, 0.5) * 1000.0).round() as u64;

    let request = RiskRequest {
        collateral: Nat::from(coll_usd as u64),
        borrowed: Nat::from(borrowed_usd as u64),
        deposits: Nat::from(deposits_usd as u64),
        volatility: Nat::from(scaled_vol),
        credit_score: Nat::from(account.credit_score.0.clone()),
    };

    let result: Result<(RiskResponse,), _> = call(principal, "risk", (request,)).await;

    if let Ok((resp,)) = result {
        account.risk_advice = Some(resp.advice.clone());
        Some(resp)
    } else {
        account.risk_advice = Some("AI service unavailable".to_string());
        None
    }
}

// ---------------- HELPER: LOG MINT ----------------
fn log_mint(pool: &mut DeFiPool, user: &str, token: &str, amount: &Nat) {
    pool.mint_logs.push((user.to_string(), token.to_string(), amount.clone()));
    pool.per_user_mint_logs
        .entry(user.to_string())
        .or_default()
        .push((token.to_string(), amount.clone()));

    ic_cdk::print(format!(
        "log_mint: user={}, token={}, amount={}",
        user, token, amount
    ));
}

// ---------------- DEPOSIT ----------------
#[update]
async fn deposit(token: String, amount: Nat) -> bool {
    let caller = ic_cdk::caller();

    // Get token canister principal safely
    let principal = {
        let pool = POOL.lock().unwrap();
        match pool.token_canisters.get(&token) {
            Some(p) => *p,
            None => {
                ic_cdk::print(format!("Deposit failed: token {} not supported", token));
                return false;
            }
        }
    };

    let canister_id = canister_self();

    ic_cdk::print(format!(
        "Deposit called: caller={}, token={}, amount={}, pool={}",
        caller, token, amount, canister_id
    ));

    // Step 1: Transfer token from caller to pool canister
    let transferred = dip20::transfer(principal, caller, canister_id, amount.clone()).await;
    if !transferred {
        ic_cdk::print("Deposit failed: transferFrom returned false");
        return false;
    }
    ic_cdk::print("Transfer successful");

    // Step 2: Mint stablecoin to caller
    let minted = dip20::mint(principal, caller, amount.clone()).await;
    if !minted {
        ic_cdk::print("Deposit failed: mint returned false");
        return false;
    }
    ic_cdk::print("Mint successful");

    // Step 3: Update balances and log mint inside one mutex lock
    {
        let mut pool = POOL.lock().unwrap();
        let caller_text = caller.to_text();
        let balances = pool.stablecoin_balances.entry(caller_text.clone()).or_default();
        let entry = balances.entry(token.clone()).or_insert(Nat::from(0u64));
        *entry = Nat::from(&entry.0 + &amount.0);

        log_mint(&mut pool, &caller_text, &token, &amount);
    }

    ic_cdk::print(format!(
        "Deposit successful: caller={}, token={}, amount={}",
        caller, token, amount
    ));
    true
}

// ---------------- WITHDRAW COLLATERAL ----------------
#[update]
fn withdraw_collateral(user: String, token: String, amount: Nat) -> bool {
    let mut pool = POOL.lock().unwrap();
    let user_coll = pool.collateral.entry(user.clone()).or_default();
    let coll = user_coll.entry(token.clone()).or_insert(Nat::from(0u64));
    if *coll < amount { return false; }
    let diff = &coll.0 - &amount.0;
    *coll = Nat::from(diff);
    true
}

// ---------------- BORROW ----------------
#[update]
async fn borrow(token: String, amount: Nat) -> bool {
    let caller = ic_cdk::caller();

    // Step 1: Get collateral, borrowed, and deposits for risk check
    let (coll_clone, borrowed_clone, deposits_clone) = {
        let pool = POOL.lock().unwrap();
        let coll = pool.collateral.get(&caller.to_text()).cloned().unwrap_or_default();
        let borrowed = pool.stablecoin_balances.get(&caller.to_text()).cloned().unwrap_or_default();
        let deposits = pool.stablecoin_balances.get(&caller.to_text()).cloned().unwrap_or_default();
        (coll, borrowed, deposits)
    };

    let coll_usd = aggregate_collateral(&coll_clone);
    let borrowed_usd = aggregate_borrowed(&borrowed_clone);
    let deposits_usd = aggregate_deposits(&deposits_clone);

    // Step 2: Risk check with AI
    let mut pool = POOL.lock().unwrap();
    let account = match pool.users.get_mut(&caller.to_text()) {
        Some(acc) => acc,
        None => return false,
    };
    if risk_check(account, coll_usd, borrowed_usd, deposits_usd).await.is_none() {
        return false;
    }

    // Step 3: Update borrowed balances
    let balances = pool.stablecoin_balances.entry(caller.to_text()).or_default();
    let entry = balances.entry(token.clone()).or_insert(Nat::from(0u64));
    *entry = Nat::from(&entry.0 + &amount.0);

    // Step 4: Mint token to caller
    if let Some(token_principal) = pool.token_canisters.get(&token) {
        dip20::mint(*token_principal, caller, amount.clone()).await;
        log_mint(&mut pool, &caller.to_text(), &token, &amount);
    }

    true
}


// ---------------- REPAY ----------------
#[update]
fn repay(token: String, amount: Nat) -> bool {
    let caller = ic_cdk::caller();

    let mut pool = POOL.lock().unwrap();
    let balances = pool.stablecoin_balances.entry(caller.to_text()).or_default();
    let entry = balances.entry(token.clone()).or_insert(Nat::from(0u64));

    if *entry < amount {
        return false; // cannot repay more than borrowed
    }

    let diff = &entry.0 - &amount.0;
    *entry = Nat::from(diff);

    true
}


// ---------------- DEPOSIT COLLATERAL (caller-centric) ----------------
#[update]
async fn deposit_collateral(token: String, amount: Nat) -> bool {
    let caller = ic_cdk::caller();

    // Step 1: Update user collateral inside mutex
    {
        let mut pool = POOL.lock().unwrap();
        let user_coll = pool.collateral.entry(caller.to_text()).or_default();
        let coll = user_coll.entry(token.clone()).or_insert(Nat::from(0u64));
        *coll = Nat::from(&coll.0 + &amount.0);
    }

    // Step 2: Risk check
    let (coll_clone, borrowed_clone, deposits_clone) = {
        let pool = POOL.lock().unwrap();
        let coll = pool.collateral.get(&caller.to_text()).cloned().unwrap_or_default();
        let borrowed = pool.stablecoin_balances.get(&caller.to_text()).cloned().unwrap_or_default();
        let deposits = pool.stablecoin_balances.get(&caller.to_text()).cloned().unwrap_or_default();
        (coll, borrowed, deposits)
    };

    let coll_usd = aggregate_collateral(&coll_clone);
    let borrowed_usd = aggregate_borrowed(&borrowed_clone);
    let deposits_usd = aggregate_deposits(&deposits_clone);

    let mut pool = POOL.lock().unwrap();
    if let Some(account) = pool.users.get_mut(&caller.to_text()) {
        risk_check(account, coll_usd, borrowed_usd, deposits_usd).await;
    }

    true
}

// ---------------- CROWDFUND (caller-centric) ----------------
#[update]
async fn contribute_crowdfund(token: String, amount: Nat) -> bool {
    let caller = ic_cdk::caller();

    // Step 1: Update crowdfunding pool inside mutex
    {
        let mut cf = CF_POOL.lock().unwrap();
        let total = cf.funds.entry(token.clone()).or_insert(Nat::from(0u64));
        *total = Nat::from(&total.0 + &amount.0);

        let contribs = cf.contributors.entry(caller.to_text()).or_default();
        let entry = contribs.entry(token.clone()).or_insert(Nat::from(0u64));
        *entry = Nat::from(&entry.0 + &amount.0);
    }

    // Step 2: Mint tokens outside mutex
    let token_principal_opt = {
        let pool = POOL.lock().unwrap();
        pool.token_canisters.get(&token).cloned()
    };

    if let Some(token_principal) = token_principal_opt {
        let minted = dip20::mint(token_principal, caller, amount.clone()).await;
        if minted {
            let mut pool = POOL.lock().unwrap();
            log_mint(&mut pool, &caller.to_text(), &token, &amount);
        }
    }

    true
}

// ---------------- QUERIES ----------------
#[query]
fn get_crowdfund_status() -> Vec<CrowdfundEntry> {
    let cf = CF_POOL.lock().unwrap();
    let mut entries = vec![];
    for (user, contribs) in cf.contributors.iter() {
        for (token, amt) in contribs.iter() {
            entries.push(CrowdfundEntry {
                user: user.clone(),
                token: token.clone(),
                amount: amt.clone(),
            });
        }
    }
    entries
}

#[query]
fn get_stable_token() -> StableToken {
    let pool = POOL.lock().unwrap();
    let mut balances = vec![];
    for (_user, user_balances) in pool.stablecoin_balances.iter() {
        for (token, amt) in user_balances.iter() {
            balances.push(StableBalanceEntry {
                token: token.clone(),
                value: amt.clone(),
            });
        }
    }
    let total_supply = compute_total_supply(&pool);
    StableToken {
        total_supply,
        balances,
    }
}

#[query]
fn get_user_account(user: String) -> Option<UserAccount> {
    let pool = POOL.lock().unwrap();
    pool.users.get(&user).cloned()
}

#[query]
fn get_user_balances(user: String) -> Vec<StableBalanceEntry> {
    let pool = POOL.lock().unwrap();
    let mut result = vec![];
    if let Some(balances) = pool.stablecoin_balances.get(&user) {
        for (token, amt) in balances.iter() {
            result.push(StableBalanceEntry {
                token: token.clone(),
                value: amt.clone(),
            });
        }
    }
    result
}

#[query]
fn get_user_collateral(user: String) -> Option<HashMap<String, Nat>> {
    let pool = POOL.lock().unwrap();
    pool.collateral.get(&user).cloned()
}

#[query]
fn get_balance(user: String, token: String) -> Nat {
    let pool = POOL.lock().unwrap();
    pool.stablecoin_balances
        .get(&user)
        .and_then(|m| m.get(&token))
        .cloned()
        .unwrap_or(Nat::from(0u64))
}

#[query]
fn supported_tokens() -> Vec<String> {
    let pool = POOL.lock().unwrap();
    pool.supported_tokens.clone()
}

#[query]
fn version() -> String {
    "DeFi Pool Backend v1.0.0".to_string()
}

#[query]
fn get_mint_logs() -> Vec<(String, String, Nat)> {
    let pool = POOL.lock().unwrap();
    pool.mint_logs.clone()
}

#[query]
fn get_per_user_mint_logs(user: String) -> Vec<(String, Nat)> {
    let pool = POOL.lock().unwrap();
    pool.per_user_mint_logs.get(&user).cloned().unwrap_or_default()
}
