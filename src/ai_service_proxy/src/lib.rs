// src/ai_service_proxy/lib.rs
use ic_cdk_macros::{init, query, update};
mod types;
use types::{RiskRequest, RiskResponse};
use num_traits::cast::ToPrimitive;

/// Logistic Regression Brain using exact numbers from model.pkl
struct LogisticRegressionBrain {
    means: [f64; 5],
    stds: [f64; 5],
    weights: [f64; 5],
    intercept: f64,
}

impl LogisticRegressionBrain {
    /// Standardize feature: (x - mean) / std
    fn scale(&self, x: &[f64; 5]) -> [f64; 5] {
        let mut scaled = [0.0; 5];
        for i in 0..5 {
            scaled[i] = (x[i] - self.means[i]) / self.stds[i];
        }
        scaled
    }

    /// Compute probability using sigmoid
    fn predict_proba(&self, x: &[f64; 5]) -> f64 {
        let scaled = self.scale(x);
        let mut z = self.intercept;
        for i in 0..5 {
            z += self.weights[i] * scaled[i];
        }
        1.0 / (1.0 + (-z).exp())
    }

    /// Predict class 0 = safe, 1 = high risk
    fn predict(&self, x: &[f64; 5]) -> u8 {
        let prob = self.predict_proba(x);
        if prob >= 0.5 { 1 } else { 0 }
    }
}

// Initialize brain with updated 2.5M-user model constants
static BRAIN: LogisticRegressionBrain = LogisticRegressionBrain {
    means: [0.254960, 774717.027074, 499839.415540, 1000172.144719, 574.696362],
    stds: [0.141482, 418514.422291, 288655.995022, 577065.613148, 158.832794],
    weights: [1.893918, -1.209705, 0.795901, 0.000843, -1.698044],
    intercept: 2.262179,
};

#[init]
fn init() {
    ic_cdk::println!("AI Service Proxy Initialized with Logistic Regression Brain");
}

/// Compute risk based on request
#[update]
fn risk(req: RiskRequest) -> RiskResponse {
    let features = [
        req.volatility.0.to_f64().unwrap_or(f64::MAX) / 1000.0, // scale back
        req.collateral.0.to_f64().unwrap_or(f64::MAX),
        req.borrowed.0.to_f64().unwrap_or(f64::MAX),
        req.deposits.0.to_f64().unwrap_or(f64::MAX),
        req.credit_score.0.to_f64().unwrap_or(f64::MAX),
    ];
    ic_cdk::println!("Features: {:?}", features);

    let pred = BRAIN.predict(&features);
    let prob = BRAIN.predict_proba(&features);

    let advice = if pred == 0 {
        "Safe to borrow".to_string()
    } else {
        format!("High risk (prob {:.2}), consider increasing collateral", prob)
    };

    RiskResponse { risk_score: pred, advice }
}

#[query]
fn version() -> String {
    "ai_service_proxy v1.0.0".to_string()
}
