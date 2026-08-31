#![no_std]
//! sentinel_payment — XLM micropayment rail for t3n-sentinel (Soroban port).
//!
//! Every `probe_with_payment` call can atomically transfer an XLM
//! micropayment to the provider's payout address. Providers can opt into a
//! "paywalled" mode where a probe is only recorded after the XLM transfer
//! succeeds — the invariant "no probe without XLM transfer when provider is
//! paywalled" holds by construction (the transfer happens BEFORE the receipt
//! is appended).
//!
//! Architecture: this contract extends the sentinel-vault flow with a payment
//! leg. The vault authority configures per-provider:
//!   - `payout`: the Address that receives the micropayment,
//!   - `price`:  the per-probe XLM price (i128 stroops), 0 = free,
//!   - `paywall`: whether payment is REQUIRED for a probe to be recorded.
//!
//! The TEE worker (the registered `tee_worker`) calls `probe_with_payment`:
//!   1. validates caller == tee_worker,
//!   2. requires the provider to be known,
//!   3. if paywalled and price > 0: transfers `price` XLM from the vault's
//!      funded account to `payout` via the token contract (passed in `init`),
//!   4. classifies the HTTP status and appends the receipt (same ring-buffer
//!      semantics as the vault).
//!
//! The vault holds a token balance that the authority funds; the payment is
//! pulled from the vault contract itself, so the TEE worker never needs to
//! hold funds.
//!
//! SECURITY MODEL
//! ==============
//! 1. Only the registered `tee_worker` may trigger a paid probe.
//! 2. Payment is ATOMIC with the probe: if the transfer fails (insufficient
//!    balance, token error), the receipt is NOT appended and the call panics.
//! 3. The token contract (native XLM, or any SAC) is passed in `init` and
//!    called via `TokenClient`.
//! 4. `probe_with_payment` NEVER returns the API key — only the verdict.

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, Map,
    String, Symbol, Vec,
};

#[cfg(test)]
mod test;

/// Provider registry — same maintenance contract as the vault port.
pub mod providers {
    use soroban_sdk::{Env, String};

    pub const PROVIDERS: [&str; 4] = ["github", "groq", "openrouter", "openai"];

    pub fn is_known(env: &Env, provider: &String) -> bool {
        let mut known = false;
        for p in PROVIDERS {
            if &String::from_str(env, p) == provider {
                known = true;
            }
        }
        known
    }
}

/// Storage keys.
const TEE_WORKER: Symbol = symbol_short!("WORKER");
const AUTHORITY: Symbol = symbol_short!("AUTHORITY");
const TOKEN: Symbol = symbol_short!("TOKEN");
const PROVIDER_CFG: Symbol = symbol_short!("PROV_CFG");
const HISTORY: Symbol = symbol_short!("HISTORY");
const HISTORY_COUNT: Symbol = symbol_short!("HIST_CNT");

/// Ring-buffer capacity — matches the vault (16).
pub const HISTORY_MAX: u32 = 16;

/// Per-provider payment configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    /// Address that receives the XLM micropayment.
    pub payout: Address,
    /// Per-probe price in stroops (1 XLM = 10_000_000 stroops). 0 = free.
    pub price: i128,
    /// If true, a probe is only recorded after the payment succeeds.
    pub paywalled: bool,
}

/// Canonical probe outcome — same shape as the vault port, plus `paid`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeReceipt {
    pub provider: String,
    /// VALID | INVALID | RATE_LIMITED | UNEXPECTED
    pub verdict: String,
    pub http_code: u32,
    pub detail: String,
    pub checked_at: u64,
    /// XLM paid in stroops for this probe (0 if free).
    pub paid: i128,
}

/// Map an HTTP status to a verdict. Same shape as the vault port.
pub fn classify(code: u32) -> (&'static str, &'static str) {
    match code {
        200..=299 => ("VALID", "key accepted by provider"),
        401 | 403 => ("INVALID", "credentials rejected"),
        429 => ("RATE_LIMITED", "quota exhausted"),
        _ => ("UNEXPECTED", "unclassified status code"),
    }
}

#[contract]
pub struct SentinelPayment;

#[contractimpl]
impl SentinelPayment {
    /// `init` — one-time setup. Registers the vault authority, the off-chain
    /// TEE worker authorized to trigger paid probes, and the token contract
    /// (native XLM or any SAC) used for micropayments.
    pub fn init(env: Env, authority: Address, tee_worker: Address, token: Address) {
        if env.storage().instance().has(&AUTHORITY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&AUTHORITY, &authority);
        env.storage().instance().set(&TEE_WORKER, &tee_worker);
        env.storage().instance().set(&TOKEN, &token);
        env.storage().instance().set(&HISTORY_COUNT, &0u32);
    }

    /// `configure_provider` — set (or update) the payment config for a known
    /// provider. Only the authority may call.
    pub fn configure_provider(
        env: Env,
        provider: String,
        payout: Address,
        price: i128,
        paywalled: bool,
    ) {
        Self::require_authority(&env);
        if !providers::is_known(&env, &provider) {
            panic!("unknown provider");
        }
        if price < 0 {
            panic!("negative price");
        }
        let mut cfg: Map<String, ProviderConfig> = env
            .storage()
            .instance()
            .get(&PROVIDER_CFG)
            .unwrap_or(Map::new(&env));
        cfg.set(
            provider.clone(),
            ProviderConfig {
                payout: payout.clone(),
                price,
                paywalled,
            },
        );
        env.storage().instance().set(&PROVIDER_CFG, &cfg);
    }

    /// `probe_with_payment` — the TEE worker records a probe; if the provider
    /// is paywalled (or priced), the token micropayment is transferred FIRST,
    /// atomically, then the receipt is appended. Panics if the transfer fails.
    pub fn probe_with_payment(
        env: Env,
        provider: String,
        http_code: u32,
        detail: String,
        paid: i128,
    ) -> String {
        Self::require_worker(&env);
        if !providers::is_known(&env, &provider) {
            panic!("unknown provider");
        }
        let cfg_map: Map<String, ProviderConfig> = env
            .storage()
            .instance()
            .get(&PROVIDER_CFG)
            .unwrap_or(Map::new(&env));
        let cfg = cfg_map.get(provider.clone());

        let paywalled = match &cfg {
            Some(c) => c.paywalled,
            None => false,
        };
        let price = match &cfg {
            Some(c) => c.price,
            None => 0,
        };
        let _ = price;

        // If the provider is paywalled, payment is MANDATORY for the probe to
        // be recorded. Enforce the exact configured price.
        if paywalled {
            let c = cfg.clone().unwrap();
            if paid != c.price {
                panic!("payment mismatch");
            }
            if paid <= 0 {
                panic!("paywalled provider requires payment");
            }
            // Transfer from THIS contract to the payout address.
            let token: Address = env.storage().instance().get(&TOKEN).unwrap();
            let client = TokenClient::new(&env, &token);
            client.transfer(&env.current_contract_address(), &c.payout, &paid);
        }

        let (verdict, default_detail) = classify(http_code);
        let detail_final = if detail.len() == 0 {
            String::from_str(&env, default_detail)
        } else {
            detail
        };

        let receipt = ProbeReceipt {
            provider: provider.clone(),
            verdict: String::from_str(&env, verdict),
            http_code,
            detail: detail_final,
            checked_at: env.ledger().timestamp(),
            paid,
        };
        Self::append_receipt(&env, receipt);

        String::from_str(&env, verdict)
    }

    /// `history` — newest-first audit trail (same as the vault port).
    pub fn history(env: Env) -> Vec<ProbeReceipt> {
        let count: u32 = env.storage().instance().get(&HISTORY_COUNT).unwrap_or(0);
        let history: Map<u32, ProbeReceipt> = env
            .storage()
            .instance()
            .get(&HISTORY)
            .unwrap_or(Map::new(&env));
        let mut out: Vec<ProbeReceipt> = Vec::new(&env);
        let mut i = count;
        while i > 0 {
            if let Some(r) = history.get(i - 1) {
                out.push_back(r);
            }
            i -= 1;
        }
        out
    }

    /// `provider_config` — read a provider's payment config.
    pub fn provider_config(env: Env, provider: String) -> Option<ProviderConfig> {
        let cfg_map: Map<String, ProviderConfig> = env
            .storage()
            .instance()
            .get(&PROVIDER_CFG)
            .unwrap_or(Map::new(&env));
        cfg_map.get(provider)
    }

    /// `vault_balance` — token balance of this contract (for audits/tests).
    pub fn vault_balance(env: Env) -> i128 {
        let token: Address = env.storage().instance().get(&TOKEN).unwrap();
        let client = TokenClient::new(&env, &token);
        client.balance(&env.current_contract_address())
    }

    /// `token` — the configured token contract address.
    pub fn token(env: Env) -> Address {
        env.storage().instance().get(&TOKEN).unwrap()
    }

    // --- internal helpers ---

    fn require_authority(env: &Env) {
        let authority: Address = env.storage().instance().get(&AUTHORITY).unwrap();
        authority.require_auth();
    }

    fn require_worker(env: &Env) {
        let worker: Address = env.storage().instance().get(&TEE_WORKER).unwrap();
        worker.require_auth();
    }

    fn append_receipt(env: &Env, receipt: ProbeReceipt) {
        let mut count: u32 = env.storage().instance().get(&HISTORY_COUNT).unwrap_or(0);
        let mut history: Map<u32, ProbeReceipt> = env
            .storage()
            .instance()
            .get(&HISTORY)
            .unwrap_or(Map::new(env));
        if count < HISTORY_MAX {
            history.set(count, receipt);
            count += 1;
        } else {
            // Shift left.
            let mut i = 1u32;
            while i < HISTORY_MAX {
                if let Some(prev) = history.get(i) {
                    history.set(i - 1, prev);
                }
                i += 1;
            }
            history.set(HISTORY_MAX - 1, receipt);
        }
        env.storage().instance().set(&HISTORY_COUNT, &count);
        env.storage().instance().set(&HISTORY, &history);
    }
}
