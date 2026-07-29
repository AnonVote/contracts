use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn valid_ballot_hash(env: &Env) -> String {
    String::from_str(
        env,
        "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
    )
}

fn valid_result_hash(env: &Env) -> String {
    String::from_str(
        env,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
}

fn setup() -> (Env, AnonVoteContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AnonVoteContract);
    let client = AnonVoteContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin).unwrap();
    (env, client, admin)
}

fn setup_with_id() -> (Env, Address, AnonVoteContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AnonVoteContract);
    let client = AnonVoteContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin).unwrap();
    (env, contract_id, client, admin)
}

fn setup_uninitialized() -> (Env, AnonVoteContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AnonVoteContract);
    let client = AnonVoteContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_initialize_sets_admin() {
    let (env, client, admin) = setup();
    let stored = client.get_admin().unwrap();
    assert_eq!(stored, admin);
    assert!(client.get_initialized_at().unwrap() > 0);
}

#[test]
fn test_initialize_twice_returns_already_initialized() {
    let (_, client, admin) = setup();
    let err = client.try_initialize(&admin).unwrap_err().unwrap();
    assert_eq!(err, ContractError::AlreadyInitialized);
}

#[test]
fn test_initialize_zero_address_returns_invalid_admin() {
    let (env, client) = setup_uninitialized();
    let zero_key = String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    let zero = Address::from_string(&zero_key);
    let err = client.try_initialize(&zero).unwrap_err().unwrap();
    assert_eq!(err, ContractError::InvalidAdminAddress);
}

#[test]
fn test_record_ballot_success() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    assert!(client.ballot_exists(&hash));
    assert_eq!(client.get_tokens_issued(&hash), Some(0));
    assert_eq!(client.get_votes_cast(&hash), Some(0));
}

#[test]
fn test_record_ballot_invalid_hash_wrong_length() {
    let (env, client, admin) = setup();
    let short = String::from_str(&env, "abc");
    let err = client
        .try_record_ballot(&admin, &short)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidBallotIdHash);
}

#[test]
fn test_record_ballot_invalid_hash_uppercase() {
    let (env, client, admin) = setup();
    let upper = String::from_str(
        &env,
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
    );
    let err = client
        .try_record_ballot(&admin, &upper)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidBallotIdHash);
}

#[test]
fn test_record_ballot_invalid_hash_non_hex() {
    let (env, client, admin) = setup();
    let non_hex = String::from_str(
        &env,
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    );
    let err = client
        .try_record_ballot(&admin, &non_hex)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidBallotIdHash);
}

#[test]
fn test_record_ballot_duplicate_returns_already_exists() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    let err = client
        .try_record_ballot(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotAlreadyExists);
}

#[test]
fn test_record_token_success() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client.record_token(&admin, &hash).unwrap();
    assert_eq!(client.get_tokens_issued(&hash), Some(1));
    client.record_token(&admin, &hash).unwrap();
    assert_eq!(client.get_tokens_issued(&hash), Some(2));
}

#[test]
fn test_record_token_ballot_not_found() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let err = client
        .try_record_token(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotNotFound);
}

#[test]
fn test_record_token_after_finalised_returns_error() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let result = valid_result_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client.record_result(&admin, &hash, &result).unwrap();
    let err = client
        .try_record_token(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotAlreadyFinalised);
}

#[test]
fn test_record_vote_success() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client.record_vote(&admin, &hash).unwrap();
    assert_eq!(client.get_votes_cast(&hash), Some(1));
    client.record_vote(&admin, &hash).unwrap();
    assert_eq!(client.get_votes_cast(&hash), Some(2));
}

#[test]
fn test_record_vote_ballot_not_found() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let err = client
        .try_record_vote(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotNotFound);
}

#[test]
fn test_record_vote_after_finalised_returns_error() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let result = valid_result_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client.record_result(&admin, &hash, &result).unwrap();
    let err = client
        .try_record_vote(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotAlreadyFinalised);
}

#[test]
fn test_counter_overflow_at_max() {
    let (env, contract_id, client, admin) = setup_with_id();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();

    let bh = hash.clone();
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TokensIssued(bh), &u32::MAX);
    });

    let err = client
        .try_record_token(&admin, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::CounterOverflow);
}

#[test]
fn test_counter_overflow_does_not_panic() {
    let (env, contract_id, client, admin) = setup_with_id();
    let hash1 = valid_ballot_hash(&env);
    let hash2 = String::from_str(
        &env,
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
    );
    client.record_ballot(&admin, &hash1).unwrap();
    client.record_ballot(&admin, &hash2).unwrap();

    let bh = hash1.clone();
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TokensIssued(bh), &u32::MAX);
    });

    client.record_token(&admin, &hash1).unwrap_err().unwrap();
    client.record_token(&admin, &hash2).unwrap();
    assert_eq!(client.get_tokens_issued(&hash2), Some(1));
}

#[test]
fn test_counter_overflow_emits_event() {
    let (env, contract_id, client, admin) = setup_with_id();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();

    let bh = hash.clone();
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TokensIssued(bh), &u32::MAX);
    });

    client.try_record_token(&admin, &hash).unwrap_err().unwrap();
    let events = env.events().all();
    assert!(events.len() >= 2);
}

#[test]
fn test_record_result_success() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let result = valid_result_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client
        .record_result(&admin, &hash, &result)
        .unwrap();
    assert_eq!(client.get_result_hash(&hash), Some(result));
    assert!(client.result_exists(&hash));
}

#[test]
fn test_record_result_invalid_hash() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let bad_result = String::from_str(&env, "nothex");
    client.record_ballot(&admin, &hash).unwrap();
    let err = client
        .try_record_result(&admin, &hash, &bad_result)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidResultHash);
}

#[test]
fn test_record_result_twice_returns_already_finalised() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let result = valid_result_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client
        .record_result(&admin, &hash, &result)
        .unwrap();
    let result2 = String::from_str(
        &env,
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    );
    let err = client
        .try_record_result(&admin, &hash, &result2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotAlreadyFinalised);
}

#[test]
fn test_record_result_ballot_not_found() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    let result = valid_result_hash(&env);
    let err = client
        .try_record_result(&admin, &hash, &result)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::BallotNotFound);
}

#[test]
fn test_rotate_admin_success() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    client.rotate_admin(&admin, &new_admin).unwrap();
    let stored = client.get_admin().unwrap();
    assert_eq!(stored, new_admin);
    let err = client
        .try_record_ballot(&admin, &valid_ballot_hash(&env))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized);
    client
        .record_ballot(&new_admin, &valid_ballot_hash(&env))
        .unwrap();
}

#[test]
fn test_rotate_admin_same_address_returns_invalid() {
    let (env, client, admin) = setup();
    let err = client
        .try_rotate_admin(&admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidAdminAddress);
}

#[test]
fn test_rotate_admin_zero_address_returns_invalid() {
    let (env, client, admin) = setup();
    let zero_key = String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    let zero = Address::from_string(&zero_key);
    let err = client
        .try_rotate_admin(&admin, &zero)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::InvalidAdminAddress);
}

#[test]
fn test_rotate_admin_unauthorized() {
    let (env, client, _admin) = setup();
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let err = client
        .try_rotate_admin(&attacker, &new_admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized);
}

#[test]
fn test_rotate_admin_history_grows() {
    let (env, client, admin) = setup();
    let new_admin1 = Address::generate(&env);
    let new_admin2 = Address::generate(&env);
    client.rotate_admin(&admin, &new_admin1).unwrap();
    client.rotate_admin(&new_admin1, &new_admin2).unwrap();
    let history = client.get_admin_history();
    assert_eq!(history.len(), 2);
    let first = history.get(0).unwrap();
    assert_eq!(first.old_admin, admin);
    assert_eq!(first.new_admin, new_admin1);
    let second = history.get(1).unwrap();
    assert_eq!(second.old_admin, new_admin1);
    assert_eq!(second.new_admin, new_admin2);
}

#[test]
fn test_is_consistent_true() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    assert!(client.is_consistent(&hash));
    client.record_token(&admin, &hash).unwrap();
    assert!(!client.is_consistent(&hash));
    client.record_vote(&admin, &hash).unwrap();
    assert!(client.is_consistent(&hash));
}

#[test]
fn test_is_consistent_false() {
    let (env, client, admin) = setup();
    let hash = valid_ballot_hash(&env);
    client.record_ballot(&admin, &hash).unwrap();
    client.record_token(&admin, &hash).unwrap();
    client.record_token(&admin, &hash).unwrap();
    client.record_vote(&admin, &hash).unwrap();
    assert!(!client.is_consistent(&hash));
}

#[test]
fn test_is_consistent_nonexistent_ballot() {
    let (env, client, _admin) = setup();
    let hash = valid_ballot_hash(&env);
    assert!(!client.is_consistent(&hash));
}
