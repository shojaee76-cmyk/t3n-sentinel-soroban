#![no_std]
//! sentinel_vault — Private API-key vault & health sentinel for Soroban AI agents.
//!
//! This contract is the Soroban port of the T3N TEE WASM contract of the same
//! name (t3n-sentinel, contract id 741 on T3N testnet) and of the Solana Anchor
//! port (t3n-sentinel-solana). The shape of the API is identical
//! (`init / seal / probe / list / rotate / history`); the storage model moves
//! from a host-bound KV map to the Soroban ledger.
//!
//! SECURITY MODEL
//! ==============
//! 1. Key material (the encrypted blob) is stored per (vault, provider) under
//!    the `SECRETS` map. The actual key material is held by a TEE worker
//!    registered in the contract; the contract holds the access policy and the
//!    audit log.
//! 2. A `tee_worker` address is the ONLY caller authorized to invoke
//!    `record_probe` (the off-chain TEE adapter that does the HTTP probe).
//! 3. `HISTORY` is an append-only ring buffer (HISTORY_MAX = 16 entries).
//! 4. The probe function NEVER returns the API key — only the verdict
//!    (VALID | INVALID | RATE_LIMITED | UNEXPECTED), matching the T3N egress
//!    shape exactly.
//!
//! MAINTENANCE CONTRACT
//! ====================
//! Adding a new provider = appending ONE entry to `providers::PROVIDERS`.
//! No schema migration, no client update. (Same as the T3N + Solana ports.)

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec};

pub mod providers;

#[cfg(test)]
mod test;

use providers::{classify, find, PROVIDERS};

/// Storage keys (instance storage).
const AUTHORITY: Symbol = symbol_short!("AUTHORITY");
const TEE_WORKER: Symbol = symbol_short!("WORKER");
const SECRETS: Symbol = symbol_short!("SECRETS");
const HISTORY: Symbol = symbol_short!("HISTORY");
const HISTORY_COUNT: Symbol = symbol_short!("HIST_CNT");

/// Ring-buffer capacity — matches HISTORY_MAX in the Solana port (16).
pub const HISTORY_MAX: u32 = 16;

/// One sealed secret per (vault, provider). Key material is stored as the
/// encrypted blob here for the testnet milestone; the decryption key lives
/// inside the TEE worker.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretEntry {
    pub provider: String,
    pub secret_blob: String,
    pub sealed_at: u64,
}

/// Canonical probe outcome — same shape as the T3N Verdict and the Solana
/// ProbeReceipt.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeReceipt {
    pub provider: String,
    /// VALID | INVALID | RATE_LIMITED | UNEXPECTED
    pub verdict: String,
    pub http_code: u32,
    pub detail: String,
    pub checked_at: u64,
}

#[contract]
pub struct SentinelVault;

#[contractimpl]
impl SentinelVault {
    /// `init` — one-time vault setup. Registers the vault authority and the
    /// off-chain TEE worker that is authorized to write probe receipts.
    /// Panics if the vault is already initialized.
    pub fn init(env: Env, authority: Address, tee_worker: Address) {
        if env.storage().instance().has(&AUTHORITY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&AUTHORITY, &authority);
        env.storage().instance().set(&TEE_WORKER, &tee_worker);
        env.storage().instance().set(&HISTORY_COUNT, &0u32);
    }

    /// `seal` — write a new API key (encrypted blob) into the vault under a
    /// provider. Only the vault authority may seal. Panics on unknown
    /// provider or empty key.
    pub fn seal(env: Env, provider: String, secret_blob: String) {
        Self::require_authority(&env);
        require_known_provider(&env, &provider);
        if secret_blob.len() == 0 {
            panic!("empty key");
        }
        let mut secrets: Map<String, SecretEntry> = env
            .storage()
            .instance()
            .get(&SECRETS)
            .unwrap_or(Map::new(&env));
        let now = env.ledger().timestamp();
        secrets.set(
            provider.clone(),
            SecretEntry {
                provider: provider.clone(),
                secret_blob: secret_blob.clone(),
                sealed_at: now,
            },
        );
        env.storage().instance().set(&SECRETS, &secrets);
    }

    /// `record_probe` — called by the registered TEE worker after running the
    /// authenticated HTTP probe off-chain. Classifies the HTTP status and
    /// appends a ProbeReceipt into the history ring buffer.
    /// Panics unless the caller is the registered TEE worker.
    pub fn record_probe(env: Env, provider: String, http_code: u32, detail: String) {
        let worker: Address = env
            .storage()
            .instance()
            .get(&TEE_WORKER)
            .unwrap_or_else(|| panic!("vault not initialized"));
        worker.require_auth();
        require_known_provider(&env, &provider);

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
        };

        // Append to the ring buffer; shift left when full.
        let mut history: Vec<ProbeReceipt> = env
            .storage()
            .instance()
            .get(&HISTORY)
            .unwrap_or(Vec::new(&env));
        if history.len() >= HISTORY_MAX {
            // Drop the oldest entry (front of the vec) to keep HISTORY_MAX.
            let mut shifted = Vec::new(&env);
            for i in 1..history.len() {
                shifted.push_back(history.get(i).unwrap());
            }
            history = shifted;
        }
        history.push_back(receipt.clone());
        env.storage().instance().set(&HISTORY, &history);
        env.storage()
            .instance()
            .set(&HISTORY_COUNT, &(history.len() as u32));
    }

    /// `list_providers` — snapshot of which providers are sealed, plus the
    /// last verdict for each. Mirrors the Solana `list_providers`.
    pub fn list_providers(env: Env) -> Vec<(String, bool, Option<ProbeReceipt>)> {
        let secrets: Map<String, SecretEntry> = env
            .storage()
            .instance()
            .get(&SECRETS)
            .unwrap_or(Map::new(&env));
        let history: Vec<ProbeReceipt> = env
            .storage()
            .instance()
            .get(&HISTORY)
            .unwrap_or(Vec::new(&env));
        let mut latest: Map<String, ProbeReceipt> = Map::new(&env);
        for r in history.iter() {
            latest.set(r.provider.clone(), r.clone());
        }
        let mut rows = Vec::new(&env);
        for p in PROVIDERS.iter() {
            let name = String::from_str(&env, p.name);
            rows.push_back((name.clone(), secrets.contains_key(name.clone()), latest.get(name)));
        }
        rows
    }

    /// `rotate` — seal a new blob over an existing provider entry. Same ACL
    /// as `seal`. Panics if the provider was never sealed.
    pub fn rotate(env: Env, provider: String, new_blob: String) {
        Self::require_authority(&env);
        require_known_provider(&env, &provider);
        if new_blob.len() == 0 {
            panic!("empty key");
        }
        let secrets: Map<String, SecretEntry> = env
            .storage()
            .instance()
            .get(&SECRETS)
            .unwrap_or(Map::new(&env));
        if !secrets.contains_key(provider.clone()) {
            panic!("provider not sealed");
        }
        let mut secrets = secrets;
        let entry = secrets.get(provider.clone()).unwrap();
        let mut entry = entry;
        entry.secret_blob = new_blob;
        entry.sealed_at = env.ledger().timestamp();
        secrets.set(provider.clone(), entry);
        env.storage().instance().set(&SECRETS, &secrets);
    }

    /// `history` — return the ring buffer's entries, newest first.
    pub fn history(env: Env) -> Vec<ProbeReceipt> {
        let history: Vec<ProbeReceipt> = env
            .storage()
            .instance()
            .get(&HISTORY)
            .unwrap_or(Vec::new(&env));
        let mut out = Vec::new(&env);
        for i in (0..history.len()).rev() {
            out.push_back(history.get(i).unwrap());
        }
        out
    }

    /// `get_secret` — fetch the encrypted blob for a provider. Only the
    /// registered TEE worker may read blobs. Panics otherwise.
    pub fn get_secret(env: Env, provider: String) -> String {
        let worker: Address = env
            .storage()
            .instance()
            .get(&TEE_WORKER)
            .unwrap_or_else(|| panic!("vault not initialized"));
        worker.require_auth();
        require_known_provider(&env, &provider);
        let secrets: Map<String, SecretEntry> = env
            .storage()
            .instance()
            .get(&SECRETS)
            .unwrap_or(Map::new(&env));
        secrets
            .get(provider)
            .unwrap_or_else(|| panic!("provider not sealed"))
            .secret_blob
    }

    /// `vault_info` — authority + tee_worker + sealed count (read-only).
    pub fn vault_info(env: Env) -> (Address, Address, u32) {
        let authority: Address = env
            .storage()
            .instance()
            .get(&AUTHORITY)
            .unwrap_or_else(|| panic!("vault not initialized"));
        let worker: Address = env
            .storage()
            .instance()
            .get(&TEE_WORKER)
            .unwrap_or_else(|| panic!("vault not initialized"));
        let secrets: Map<String, SecretEntry> = env
            .storage()
            .instance()
            .get(&SECRETS)
            .unwrap_or(Map::new(&env));
        (authority, worker, secrets.len())
    }

    fn require_authority(env: &Env) {
        let authority: Address = env
            .storage()
            .instance()
            .get(&AUTHORITY)
            .unwrap_or_else(|| panic!("vault not initialized"));
        authority.require_auth();
    }
}

fn require_known_provider(env: &Env, provider: &String) {
    let known = PROVIDERS
        .iter()
        .any(|p| String::from_str(env, p.name) == *provider);
    if !known {
        panic!("unknown provider");
    }
}
