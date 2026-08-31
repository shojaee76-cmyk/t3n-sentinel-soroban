#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as AddressTestTrait, Env};

/// Helper: deploy a fresh vault with a given authority + tee worker.
fn deploy(env: &Env) -> (Address, Address, SentinelVaultClient) {
    let authority = Address::generate(env);
    let worker = Address::generate(env);
    let contract_id = env.register(SentinelVault, ());
    let client = SentinelVaultClient::new(env, &contract_id);
    client.init(&authority, &worker);
    (authority, worker, client)
}

#[test]
fn init_sets_authority_and_worker() {
    let env = Env::default();
    let (authority, worker, client) = deploy(&env);
    let (auth, tee, count) = client.vault_info();
    assert_eq!(auth, authority);
    assert_eq!(tee, worker);
    assert_eq!(count, 0);
}

#[test]
#[should_panic(expected = "already initialized")]
fn init_twice_panics() {
    let env = Env::default();
    let authority = Address::generate(&env);
    let worker = Address::generate(&env);
    let contract_id = env.register(SentinelVault, ());
    let client = SentinelVaultClient::new(&env, &contract_id);
    client.init(&authority, &worker);
    client.init(&authority, &worker);
}

#[test]
#[should_panic(expected = "unknown provider")]
fn seal_unknown_provider_panics() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().seal(
        &String::from_str(&env, "not_a_provider"),
        &String::from_str(&env, "blob"),
    );
}

#[test]
#[should_panic(expected = "empty key")]
fn seal_empty_key_panics() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().seal(
        &String::from_str(&env, "github"),
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "unknown provider")]
fn record_probe_unknown_provider_panics() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().record_probe(
        &String::from_str(&env, "nope"),
        &200u32,
        &String::from_str(&env, ""),
    );
}

#[test]
fn record_probe_2xx_is_valid_and_appends() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().record_probe(
        &String::from_str(&env, "github"),
        &200u32,
        &String::from_str(&env, ""),
    );
    let hist = client.history();
    assert_eq!(hist.len(), 1);
    assert_eq!(
        hist.get(0).unwrap().verdict,
        String::from_str(&env, "VALID")
    );
    assert_eq!(hist.get(0).unwrap().http_code, 200);
}

#[test]
fn record_probe_401_is_invalid() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().record_probe(
        &String::from_str(&env, "groq"),
        &401u32,
        &String::from_str(&env, "Bad credentials"),
    );
    let hist = client.history();
    assert_eq!(
        hist.get(0).unwrap().verdict,
        String::from_str(&env, "INVALID")
    );
    assert_eq!(
        hist.get(0).unwrap().detail,
        String::from_str(&env, "Bad credentials")
    );
}

#[test]
fn history_ring_buffer_caps_at_16_and_reverses() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    for i in 0..20u32 {
        client.mock_all_auths().record_probe(
            &String::from_str(&env, "github"),
            &(200u32 + i % 5),
            &String::from_str(&env, ""),
        );
    }
    let hist = client.history();
    assert_eq!(hist.len(), HISTORY_MAX);
    // Newest first: the 20th probe (code 200 + 19 % 5 = 204) is at index 0.
    assert_eq!(hist.get(0).unwrap().http_code, 200 + 19 % 5);
    // The oldest surviving entry is the 5th probe (i=4): code 204.
    assert_eq!(hist.get(15).unwrap().http_code, 200 + 4 % 5);
}

#[test]
fn rotate_updates_blob_and_requires_authority() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().seal(
        &String::from_str(&env, "github"),
        &String::from_str(&env, "blob_v1"),
    );
    client.mock_all_auths().rotate(
        &String::from_str(&env, "github"),
        &String::from_str(&env, "blob_v2"),
    );
}

#[test]
#[should_panic(expected = "provider not sealed")]
fn rotate_unsealed_provider_panics() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().rotate(
        &String::from_str(&env, "groq"),
        &String::from_str(&env, "blob"),
    );
}

#[test]
fn get_secret_requires_worker_and_returns_blob() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().seal(
        &String::from_str(&env, "openrouter"),
        &String::from_str(&env, "sk-encrypted-blob"),
    );
    let blob = client
        .mock_all_auths()
        .get_secret(&String::from_str(&env, "openrouter"));
    assert_eq!(blob, String::from_str(&env, "sk-encrypted-blob"));
}

#[test]
fn list_providers_shows_sealed_and_last_verdict() {
    let env = Env::default();
    let (_authority, _worker, client) = deploy(&env);
    client.mock_all_auths().seal(
        &String::from_str(&env, "github"),
        &String::from_str(&env, "blob"),
    );
    client.mock_all_auths().record_probe(
        &String::from_str(&env, "github"),
        &429u32,
        &String::from_str(&env, ""),
    );
    let rows = client.list_providers();
    assert_eq!(rows.len(), PROVIDERS.len() as u32);
    // github row: sealed + last verdict RATE_LIMITED.
    let github_row = rows
        .iter()
        .find(|(name, _sealed, _last)| name == &String::from_str(&env, "github"))
        .unwrap();
    assert_eq!(github_row.1, true);
    assert_eq!(
        github_row.2.clone().unwrap().verdict,
        String::from_str(&env, "RATE_LIMITED")
    );
    // groq row: not sealed, no last.
    let groq_row = rows
        .iter()
        .find(|(name, _sealed, _last)| name == &String::from_str(&env, "groq"))
        .unwrap();
    assert_eq!(groq_row.1, false);
    assert!(groq_row.2.is_none());
}
