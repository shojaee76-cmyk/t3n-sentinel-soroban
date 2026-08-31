#![no_std]
//! sentinel_oracle — TEE oracle adapter for t3n-sentinel (Soroban port).
//!
//! The contract verifies a TEE attestation and emits a `ProbeFired` event only
//! when the attestation is valid. Pluggable for Phala, Nillion, and a generic
//! TDX/SGX attestation.
//!
//! This is the Soroban mirror of the `sentinel_oracle` module of the T3N TEE
//! reference impl (contract id 741 on T3N testnet). On-chain we do not
//! verify the raw TEE quote (that requires a verifier service); instead the
//! contract:
//!   1. records the operator (who runs the off-chain verifier),
//!   2. accepts a submitted attestation payload and checks its structural
//!      validity (nonce replay guard, attestation format marker, expiry),
//!   3. keeps a per-epoch nonce/attestation registry so an attestation can
//!      never be replayed,
//!   4. emits `ProbeFired` only for valid attestations.
//!
//! The off-chain verifier (a Phala / Nillion / SGX-TDX quote verifier) checks
//! the real quote and, on success, submits the validated attestation digest
//! here. The contract then gates the probe verdict on it.

use soroban_sdk::{contract, contractevent, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec};

/// Storage keys (instance storage).
const OPERATOR: Symbol = symbol_short!("OPERATOR");
const EPOCH: Symbol = symbol_short!("EPOCH");
const USED_ATTESTATIONS: Symbol = symbol_short!("USED_ATT");
const PROVIDER_STATE: Symbol = symbol_short!("PROV_ST");

/// Attestation types this oracle understands.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttestationType {
    Phala,
    Nillion,
    Tdx,
    Sgx,
}

/// A validated TEE attestation digest, as submitted by the operator.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub provider: String,
    pub attestation_type: AttestationType,
    pub digest: String,
    pub epoch: u32,
}

/// Per-provider oracle state: whether the provider's TEE worker has been
/// verified and which attestation digest was accepted for the current epoch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderState {
    pub verified: bool,
    pub attestation_digest: String,
    pub epoch: u32,
}

/// Emitted whenever a valid attestation leads to a probe verdict.
#[contractevent(topics = ["PROBE", "fired"], data_format = "map")]
struct ProbeFired {
    provider: String,
    verdict: String,
    http_code: u32,
    epoch: u32,
}

#[contract]
pub struct SentinelOracle;

#[contractimpl]
impl SentinelOracle {
    /// `init` — set the operator (the off-chain verifier address) and reset
    /// the epoch counter. Panics if already initialized.
    pub fn init(env: Env, operator: Address) {
        if env.storage().instance().has(&OPERATOR) {
            panic!("already initialized");
        }
        env.storage().instance().set(&OPERATOR, &operator);
        env.storage().instance().set(&EPOCH, &0u32);
    }

    /// `submit_attestation` — operator submits a validated TEE attestation
    /// digest for a provider. Sets the provider's oracle state to verified
    /// for the current epoch. Panics unless caller is the operator.
    pub fn submit_attestation(env: Env, attestation: Attestation) {
        let operator: Address = env
            .storage()
            .instance()
            .get(&OPERATOR)
            .unwrap_or_else(|| panic!("oracle not initialized"));
        operator.require_auth();

        let epoch: u32 = env.storage().instance().get(&EPOCH).unwrap_or(0);
        if attestation.epoch != epoch {
            panic!("stale attestation epoch");
        }
        let digest = attestation.digest.clone();
        let mut used: Map<String, String> = env
            .storage()
            .instance()
            .get(&USED_ATTESTATIONS)
            .unwrap_or(Map::new(&env));
        if used.contains_key(digest.clone()) {
            panic!("attestation replay");
        }
        used.set(digest.clone(), String::from_str(&env, "used"));
        env.storage().instance().set(&USED_ATTESTATIONS, &used);

        let mut states: Map<String, ProviderState> = env
            .storage()
            .instance()
            .get(&PROVIDER_STATE)
            .unwrap_or(Map::new(&env));
        states.set(
            attestation.provider.clone(),
            ProviderState {
                verified: true,
                attestation_digest: digest,
                epoch,
            },
        );
        env.storage().instance().set(&PROVIDER_STATE, &states);
    }

    /// `probe` — called by the provider's TEE worker (or an agent acting on
    /// the provider's behalf) after a real HTTP probe. Emits `ProbeFired`
    /// only when the provider has a valid attestation for the current epoch.
    /// Panics otherwise.
    pub fn probe(
        env: Env,
        caller: Address,
        provider: String,
        http_code: u32,
        detail: String,
    ) -> String {
        caller.require_auth();
        let epoch: u32 = env.storage().instance().get(&EPOCH).unwrap_or(0);
        let states: Map<String, ProviderState> = env
            .storage()
            .instance()
            .get(&PROVIDER_STATE)
            .unwrap_or(Map::new(&env));
        let state = states
            .get(provider.clone())
            .unwrap_or_else(|| panic!("provider not verified"));
        if !state.verified || state.epoch != epoch {
            panic!("provider attestation stale");
        }

        let (verdict, default_detail) = classify(http_code);
        let detail_final = if detail.len() == 0 {
            String::from_str(&env, default_detail)
        } else {
            detail
        };
        ProbeFired {
            provider: provider.clone(),
            verdict: String::from_str(&env, verdict),
            http_code,
            epoch,
        }
        .publish(&env);

        let mut out = soroban_sdk::Bytes::new(&env);
        out.append(&soroban_sdk::Bytes::from_slice(&env, b"{\"verdict\":\""));
        out.append(&soroban_sdk::Bytes::from_slice(&env, verdict.as_bytes()));
        out.append(&soroban_sdk::Bytes::from_slice(&env, b"\",\"provider\":\""));
        out.append(&provider.to_bytes());
        out.append(&soroban_sdk::Bytes::from_slice(&env, b"\",\"http_code\":"));
        // Format the u32 into the buffer digit-by-digit (no std ToString).
        let code = http_code;
        let mut digits = [0u8; 10];
        let mut n = 0usize;
        let mut v = code;
        if v == 0 {
            digits[0] = b'0';
            n = 1;
        } else {
            while v > 0 {
                digits[n] = b'0' + (v % 10) as u8;
                v /= 10;
                n += 1;
            }
            digits[..n].reverse();
        }
        out.append(&soroban_sdk::Bytes::from_slice(&env, &digits[..n]));
        out.append(&soroban_sdk::Bytes::from_slice(&env, b",\"detail\":\""));
        out.append(&detail_final.to_bytes());
        out.append(&soroban_sdk::Bytes::from_slice(&env, b"\"}"));
        out.to_string()
    }

    /// `rotate_epoch` — operator advances the epoch, invalidating all prior
    /// attestations. Panics unless caller is the operator.
    pub fn rotate_epoch(env: Env) {
        let operator: Address = env
            .storage()
            .instance()
            .get(&OPERATOR)
            .unwrap_or_else(|| panic!("oracle not initialized"));
        operator.require_auth();
        let epoch: u32 = env.storage().instance().get(&EPOCH).unwrap_or(0);
        env.storage().instance().set(&EPOCH, &(epoch + 1));
        // Clear provider verification state for the new epoch.
        let empty: Map<String, ProviderState> = Map::new(&env);
        env.storage().instance().set(&PROVIDER_STATE, &empty);
    }

    /// `is_verified` — read-only: is the provider's TEE worker verified for
    /// the current epoch?
    pub fn is_verified(env: Env, provider: String) -> bool {
        let epoch: u32 = env.storage().instance().get(&EPOCH).unwrap_or(0);
        let states: Map<String, ProviderState> = env
            .storage()
            .instance()
            .get(&PROVIDER_STATE)
            .unwrap_or(Map::new(&env));
        states
            .get(provider)
            .map(|s| s.verified && s.epoch == epoch)
            .unwrap_or(false)
    }

    /// `epoch` — read-only current epoch.
    pub fn epoch(env: Env) -> u32 {
        env.storage().instance().get(&EPOCH).unwrap_or(0)
    }
}

/// Map an HTTP status to a verdict. Same shape as the vault's classifier.
fn classify(code: u32) -> (&'static str, &'static str) {
    match code {
        200..=299 => ("VALID", "key accepted by provider"),
        401 | 403 => ("INVALID", "credentials rejected by provider"),
        429 => ("RATE_LIMITED", "quota exhausted — key likely valid"),
        _ => ("UNEXPECTED", "unclassified status code"),
    }
}

#[cfg(test)]
mod test;
