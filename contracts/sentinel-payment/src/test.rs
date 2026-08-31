#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as AddressTestTrait,
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

/// Deploy the payment contract with a fresh stellar asset contract as the
/// token. Returns (env, contract_id, token_addr, authority, worker).
fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    let authority = Address::generate(&env);
    let worker = Address::generate(&env);
    let contract_id = env.register(SentinelPayment, ());
    let client = SentinelPaymentClient::new(&env, &contract_id);

    // Register a stellar asset contract (SAC) to act as XLM.
    let sac = env.register_stellar_asset_contract_v2(authority.clone());
    let token_addr = sac.address();

    client.init(&authority, &worker, &token_addr);
    (env, contract_id, token_addr, authority, worker)
}

/// Fund the payment contract with `amount` tokens.
/// The SAC is registered per-admin; calling register again with the same
/// admin returns the same token address (idempotent in the test env).
fn fund(env: &Env, token_addr: &Address, contract_id: &Address, authority: &Address, amount: i128) {
    let sac = StellarAssetClient::new(env, token_addr);
    sac.mock_all_auths().mint(contract_id, &amount);
}

#[test]
fn init_sets_authority_worker_token() {
    let (env, contract_id, token_addr, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    assert_eq!(client.token(), token_addr);
    assert_eq!(client.vault_balance(), 0);
    let _ = (authority, worker);
}

#[test]
#[should_panic(expected = "already initialized")]
fn init_twice_panics() {
    let env = Env::default();
    let authority = Address::generate(&env);
    let worker = Address::generate(&env);
    let contract_id = env.register(SentinelPayment, ());
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let sac = env.register_stellar_asset_contract_v2(authority.clone());
    let token_addr = sac.address();
    client.init(&authority, &worker, &token_addr);
    client.init(&authority, &worker, &token_addr);
}

#[test]
fn configure_provider_sets_config() {
    let (env, contract_id, _token, authority, _worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);

    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &1_000_000,
        &true,
    );
    let cfg = client
        .provider_config(&String::from_str(&env, "github"))
        .unwrap();
    assert_eq!(cfg.payout, payout);
    assert_eq!(cfg.price, 1_000_000);
    assert!(cfg.paywalled);
    let _ = authority;
}

#[test]
#[should_panic(expected = "unknown provider")]
fn configure_unknown_provider_panics() {
    let (env, contract_id, _token, authority, _worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);
    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "nope"),
        &payout,
        &100,
        &true,
    );
    let _ = authority;
}

#[test]
fn free_probe_records_without_payment() {
    let (env, contract_id, _token, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);
    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &0,
        &false,
    );

    let v = client.mock_all_auths().probe_with_payment(
        &String::from_str(&env, "github"),
        &200,
        &String::from_str(&env, ""),
        &0,
    );
    assert_eq!(v, String::from_str(&env, "VALID"));

    let h = client.history();
    assert_eq!(h.len(), 1);
    let r = h.get(0).unwrap();
    assert_eq!(r.verdict, String::from_str(&env, "VALID"));
    assert_eq!(r.paid, 0);
    let _ = (authority, worker);
}

#[test]
fn paywalled_probe_transfers_and_records() {
    let (env, contract_id, token_addr, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);

    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &1_000_000,
        &true,
    );

    // Fund the contract with 10 XLM (10_000_000 stroops * 10)
    fund(&env, &token_addr, &contract_id, &authority, 10_000_000);

    let v = client.mock_all_auths().probe_with_payment(
        &String::from_str(&env, "github"),
        &200,
        &String::from_str(&env, ""),
        &1_000_000,
    );
    assert_eq!(v, String::from_str(&env, "VALID"));

    // Payout received the payment
    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&payout), 1_000_000);
    // Contract balance reduced
    assert_eq!(token.balance(&contract_id), 9_000_000);

    // Receipt recorded with paid amount
    let h = client.history();
    let r = h.get(0).unwrap();
    assert_eq!(r.paid, 1_000_000);
    let _ = worker;
}

#[test]
#[should_panic(expected = "caught panic 'payment mismatch'")]
fn paywalled_probe_wrong_amount_panics() {
    let (env, contract_id, token_addr, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);

    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &1_000_000,
        &true,
    );
    fund(&env, &token_addr, &contract_id, &authority, 10_000_000);

    // Try to pay 500k instead of 1M → must panic
    client.mock_all_auths().probe_with_payment(
        &String::from_str(&env, "github"),
        &200,
        &String::from_str(&env, ""),
        &500_000,
    );
    let _ = worker;
}

#[test]
#[should_panic(expected = "caught panic 'payment mismatch'")]
fn paywalled_probe_zero_payment_panics() {
    let (env, contract_id, _token, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);

    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &1_000_000,
        &true,
    );

    client.mock_all_auths().probe_with_payment(
        &String::from_str(&env, "github"),
        &200,
        &String::from_str(&env, ""),
        &0,
    );
    let _ = (authority, worker);
}

#[test]
#[should_panic(expected = "zero balance is not sufficient to spend")]
fn paywalled_probe_insufficient_balance_panics() {
    let (env, contract_id, _token, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);

    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &10_000_000,
        &true,
    );
    // No funding → transfer fails → panic, and no receipt appended.
    client.mock_all_auths().probe_with_payment(
        &String::from_str(&env, "github"),
        &200,
        &String::from_str(&env, ""),
        &10_000_000,
    );
    let _ = (authority, worker);
}

#[test]
fn history_ring_buffer_caps_at_16() {
    let (env, contract_id, _token, authority, worker) = setup();
    let client = SentinelPaymentClient::new(&env, &contract_id);
    let payout = Address::generate(&env);
    client.mock_all_auths().configure_provider(
        &String::from_str(&env, "github"),
        &payout,
        &0,
        &false,
    );

    for i in 0..20u32 {
        client.mock_all_auths().probe_with_payment(
            &String::from_str(&env, "github"),
            &(200 + i),
            &String::from_str(&env, ""),
            &0,
        );
    }
    let h = client.history();
    assert_eq!(h.len(), 16);
    // Newest first: last recorded code 219
    assert_eq!(h.get(0).unwrap().http_code, 219);
    // Oldest survivor: code 204
    assert_eq!(h.get(15).unwrap().http_code, 204);
    let _ = (authority, worker);
}

#[test]
fn classify_consistent_with_vault() {
    assert_eq!(classify(200), ("VALID", "key accepted by provider"));
    assert_eq!(classify(401), ("INVALID", "credentials rejected"));
    assert_eq!(classify(429), ("RATE_LIMITED", "quota exhausted"));
    assert_eq!(classify(500), ("UNEXPECTED", "unclassified status code"));
}
