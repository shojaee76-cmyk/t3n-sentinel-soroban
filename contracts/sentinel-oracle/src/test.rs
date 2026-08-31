#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Events},
    Env, IntoVal,
};

fn deploy(env: &Env) -> (Address, SentinelOracleClient) {
    let operator = Address::generate(env);
    let contract_id = env.register(SentinelOracle, ());
    let client = SentinelOracleClient::new(env, &contract_id);
    client.init(&operator);
    (operator, client)
}

fn att(env: &Env, provider: &str, typ: AttestationType, digest: &str, epoch: u32) -> Attestation {
    Attestation {
        provider: String::from_str(env, provider),
        attestation_type: typ,
        digest: String::from_str(env, digest),
        epoch,
    }
}

#[test]
fn init_sets_operator_and_epoch_zero() {
    let env = Env::default();
    let (operator, client) = deploy(&env);
    assert_eq!(client.epoch(), 0);
    // is_verified on an unverified provider is false.
    assert_eq!(
        client.is_verified(&String::from_str(&env, "github")),
        false
    );
    let _ = operator;
}

#[test]
#[should_panic(expected = "already initialized")]
fn init_twice_panics() {
    let env = Env::default();
    let operator = Address::generate(&env);
    let contract_id = env.register(SentinelOracle, ());
    let client = SentinelOracleClient::new(&env, &contract_id);
    client.init(&operator);
    client.init(&operator);
}

#[test]
#[should_panic(expected = "oracle not initialized")]
fn submit_before_init_panics() {
    let env = Env::default();
    let operator = Address::generate(&env);
    let contract_id = env.register(SentinelOracle, ());
    let client = SentinelOracleClient::new(&env, &contract_id);
    client.submit_attestation(&att(&env, "github", AttestationType::Sgx, "digest1", 0));
    let _ = operator;
}

#[test]
fn submit_valid_attestation_verifies_provider() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Phala,
        "digest-abc",
        0,
    ));
    assert_eq!(
        client.is_verified(&String::from_str(&env, "github")),
        true
    );
}

#[test]
#[should_panic(expected = "stale attestation epoch")]
fn submit_stale_epoch_panics() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Tdx,
        "digest-old",
        7,
    ));
}

#[test]
#[should_panic(expected = "attestation replay")]
fn submit_replay_digest_panics() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Nillion,
        "digest-same",
        0,
    ));
    // Same digest, same epoch → replay.
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "groq",
        AttestationType::Nillion,
        "digest-same",
        0,
    ));
}

#[test]
#[should_panic(expected = "provider not verified")]
fn probe_unverified_provider_panics() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    let caller = Address::generate(&env);
    client.mock_all_auths().probe(
        &caller,
        &String::from_str(&env, "openai"),
        &200u32,
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "provider not verified")]
fn probe_after_epoch_rotation_panics() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Sgx,
        "digest-e1",
        0,
    ));
    client.mock_all_auths().rotate_epoch();
    let caller = Address::generate(&env);
    client.mock_all_auths().probe(
        &caller,
        &String::from_str(&env, "github"),
        &200u32,
        &String::from_str(&env, ""),
    );
}

#[test]
fn probe_valid_emits_probefired_and_returns_verdict() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Phala,
        "digest-ok",
        0,
    ));
    let caller = Address::generate(&env);
    let out = client.mock_all_auths().probe(
        &caller,
        &String::from_str(&env, "github"),
        &200u32,
        &String::from_str(&env, ""),
    );
    // Compare by bytes to avoid String::contains.
    let out_bytes = out.to_bytes();
    let needle = b"\"verdict\":\"VALID\"";
    let mut found = false;
    for i in 0..out_bytes.len().saturating_sub(needle.len() as u32) + 1 {
        let mut eq = true;
        for j in 0..needle.len() {
            if out_bytes.get(i + j as u32) != Some(needle[j]) {
                eq = false;
                break;
            }
        }
        if eq {
            found = true;
            break;
        }
    }
    assert!(found, "expected VALID verdict in out");
    // ProbeFired event was emitted.
    let events = env.events().all();
    assert_eq!(events.events().len(), 1);
}

#[test]
fn probe_401_emits_invalid_verdict() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "groq",
        AttestationType::Nillion,
        "digest-groq",
        0,
    ));
    let caller = Address::generate(&env);
    let out = client.mock_all_auths().probe(
        &caller,
        &String::from_str(&env, "groq"),
        &401u32,
        &String::from_str(&env, "Bad credentials"),
    );
    let out_bytes = out.to_bytes();
    let needle = b"\"verdict\":\"INVALID\"";
    let mut found = false;
    for i in 0..out_bytes.len().saturating_sub(needle.len() as u32) + 1 {
        let mut eq = true;
        for j in 0..needle.len() {
            if out_bytes.get(i + j as u32) != Some(needle[j]) {
                eq = false;
                break;
            }
        }
        if eq {
            found = true;
            break;
        }
    }
    assert!(found, "expected INVALID verdict in out");
    let needle2 = b"Bad credentials";
    let mut found2 = false;
    for i in 0..out_bytes.len().saturating_sub(needle2.len() as u32) + 1 {
        let mut eq = true;
        for j in 0..needle2.len() {
            if out_bytes.get(i + j as u32) != Some(needle2[j]) {
                eq = false;
                break;
            }
        }
        if eq {
            found2 = true;
            break;
        }
    }
    assert!(found2, "expected detail in out");
}

#[test]
fn rotate_epoch_invalidates_attestations_and_bumps() {
    let env = Env::default();
    let (_operator, client) = deploy(&env);
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Sgx,
        "digest-e1",
        0,
    ));
    assert_eq!(client.epoch(), 0);
    client.mock_all_auths().rotate_epoch();
    assert_eq!(client.epoch(), 1);
    // Verification cleared.
    assert_eq!(
        client.is_verified(&String::from_str(&env, "github")),
        false
    );
    // New attestation for epoch 1 works.
    client.mock_all_auths().submit_attestation(&att(
        &env,
        "github",
        AttestationType::Sgx,
        "digest-e2",
        1,
    ));
    assert_eq!(
        client.is_verified(&String::from_str(&env, "github")),
        true
    );
}
