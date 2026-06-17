//! AnonVote Soroban Smart Contract
//!
//! Records immutable audit events on the Stellar blockchain.
//! Complements the manageData approach with on-chain queryable state.
//!
//! # What this contract does
//! - Records ballot creation events with a ballot ID hash
//! - Records token issuance counts per ballot (no voter identity)
//! - Records vote cast counts per ballot (no vote content)
//! - Records result publication with a tally hash
//! - Allows public verification of event counts on-chain
//!
//! # Privacy guarantees
//! - No voter identifiers stored
//! - No token values stored
//! - No vote content stored
//! - Only counts and hashes — same privacy model as the off-chain system

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String};

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address — only admin can record events
    Admin,
    /// Emergency pause flag — write operations are blocked while true
    Paused,
    /// Token issued count for a ballot: ballot_id_hash → u32
    TokensIssued(String),
    /// Votes cast count for a ballot: ballot_id_hash → u32
    VotesCast(String),
    /// Result hash for a ballot: ballot_id_hash → String
    ResultHash(String),
    /// Whether a ballot has been created: ballot_id_hash → bool
    BallotExists(String),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AnonVoteContract;

#[contractimpl]
impl AnonVoteContract {
    /// Initialize the contract with an admin address.
    /// Must be called once after deployment.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Pause all write operations in an emergency.
    /// Read-only audit queries continue to work while paused.
    pub fn pause_contract(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish(
            (symbol_short!("contract"), symbol_short!("paused")),
            (caller, env.ledger().timestamp()),
        );
    }

    /// Resume write operations after an emergency pause.
    pub fn resume_contract(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish(
            (symbol_short!("contract"), symbol_short!("resumed")),
            (caller, env.ledger().timestamp()),
        );
    }

    /// Record a ballot creation event.
    /// ballot_id_hash: SHA-256 hex of the ballot UUID
    pub fn record_ballot(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::require_not_paused(&env);

        let key = DataKey::BallotExists(ballot_id_hash.clone());
        if env.storage().persistent().has(&key) {
            panic!("ballot already recorded");
        }
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .set(&DataKey::TokensIssued(ballot_id_hash.clone()), &0u32);
        env.storage()
            .persistent()
            .set(&DataKey::VotesCast(ballot_id_hash), &0u32);

        env.events()
            .publish((symbol_short!("ballot"),), (symbol_short!("created"),));
    }

    /// Increment the token issued count for a ballot.
    /// Called when a voter token is issued.
    pub fn record_token(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::require_not_paused(&env);
        Self::require_ballot_exists(&env, &ballot_id_hash);

        let key = DataKey::TokensIssued(ballot_id_hash);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));

        env.events()
            .publish((symbol_short!("token"),), (symbol_short!("issued"),));
    }

    /// Increment the votes cast count for a ballot.
    /// Called when a vote is submitted.
    pub fn record_vote(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::require_not_paused(&env);
        Self::require_ballot_exists(&env, &ballot_id_hash);

        let key = DataKey::VotesCast(ballot_id_hash);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));

        env.events()
            .publish((symbol_short!("vote"),), (symbol_short!("cast"),));
    }

    /// Record the result publication for a ballot.
    /// result_hash: SHA-256 hex of the tally JSON
    pub fn record_result(env: Env, caller: Address, ballot_id_hash: String, result_hash: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::require_not_paused(&env);
        Self::require_ballot_exists(&env, &ballot_id_hash);

        let key = DataKey::ResultHash(ballot_id_hash);
        if env.storage().persistent().has(&key) {
            panic!("result already recorded");
        }
        env.storage().persistent().set(&key, &result_hash);

        env.events()
            .publish((symbol_short!("result"),), (symbol_short!("published"),));
    }

    // ── Read-only queries ────────────────────────────────────────────────────

    /// Get the number of tokens issued for a ballot.
    pub fn get_tokens_issued(env: Env, ballot_id_hash: String) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TokensIssued(ballot_id_hash))
            .unwrap_or(0)
    }

    /// Get the number of votes cast for a ballot.
    pub fn get_votes_cast(env: Env, ballot_id_hash: String) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::VotesCast(ballot_id_hash))
            .unwrap_or(0)
    }

    /// Get the result hash for a ballot (None if not yet published).
    pub fn get_result_hash(env: Env, ballot_id_hash: String) -> Option<String> {
        env.storage()
            .persistent()
            .get(&DataKey::ResultHash(ballot_id_hash))
    }

    /// Check if a ballot has been recorded on-chain.
    pub fn ballot_exists(env: Env, ballot_id_hash: String) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash))
    }

    /// Check whether write operations are currently paused.
    pub fn is_paused(env: Env) -> bool {
        Self::paused(&env)
    }

    /// Verify consistency: returns true if tokens_issued == votes_cast.
    pub fn is_consistent(env: Env, ballot_id_hash: String) -> bool {
        let tokens: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TokensIssued(ballot_id_hash.clone()))
            .unwrap_or(0);
        let votes: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::VotesCast(ballot_id_hash))
            .unwrap_or(0);
        tokens == votes
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn require_not_paused(env: &Env) {
        if Self::paused(env) {
            panic!("contract paused");
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if *caller != admin {
            panic!("unauthorized");
        }
    }

    fn require_ballot_exists(env: &Env, ballot_id_hash: &String) {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            panic!("ballot not found");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    fn setup() -> (Env, AnonVoteContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AnonVoteContract);
        let client = AnonVoteContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    #[test]
    fn test_record_ballot_and_query() {
        let (env, client, admin) = setup();
        let ballot_hash = String::from_str(&env, "abc123");
        client.record_ballot(&admin, &ballot_hash);
        assert!(client.ballot_exists(&ballot_hash));
        assert_eq!(client.get_tokens_issued(&ballot_hash), 0);
        assert_eq!(client.get_votes_cast(&ballot_hash), 0);
    }

    #[test]
    fn test_token_and_vote_counts() {
        let (env, client, admin) = setup();
        let ballot_hash = String::from_str(&env, "abc123");
        client.record_ballot(&admin, &ballot_hash);
        client.record_token(&admin, &ballot_hash);
        client.record_token(&admin, &ballot_hash);
        client.record_vote(&admin, &ballot_hash);
        assert_eq!(client.get_tokens_issued(&ballot_hash), 2);
        assert_eq!(client.get_votes_cast(&ballot_hash), 1);
        assert!(!client.is_consistent(&ballot_hash));
        client.record_vote(&admin, &ballot_hash);
        assert!(client.is_consistent(&ballot_hash));
    }

    #[test]
    fn test_record_result() {
        let (env, client, admin) = setup();
        let ballot_hash = String::from_str(&env, "abc123");
        let result_hash = String::from_str(&env, "deadbeef");
        client.record_ballot(&admin, &ballot_hash);
        client.record_result(&admin, &ballot_hash, &result_hash);
        assert_eq!(client.get_result_hash(&ballot_hash), Some(result_hash));
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_unauthorized_caller() {
        let (env, client, _admin) = setup();
        let ballot_hash = String::from_str(&env, "abc123");
        let attacker = Address::generate(&env);
        client.record_ballot(&attacker, &ballot_hash);
    }

    #[test]
    fn test_pause_and_resume_contract() {
        let (env, client, admin) = setup();
        let ballot_hash = String::from_str(&env, "paused-read-ballot");

        assert!(!client.is_paused());
        client.record_ballot(&admin, &ballot_hash);
        client.pause_contract(&admin);

        assert!(client.is_paused());
        assert!(client.ballot_exists(&ballot_hash));
        assert_eq!(client.get_tokens_issued(&ballot_hash), 0);

        client.resume_contract(&admin);
        assert!(!client.is_paused());

        client.record_token(&admin, &ballot_hash);
        assert_eq!(client.get_tokens_issued(&ballot_hash), 1);
    }

    #[test]
    #[should_panic(expected = "contract paused")]
    fn test_paused_contract_blocks_writes() {
        let (env, client, admin) = setup();
        let ballot_hash = String::from_str(&env, "paused-write-ballot");

        client.pause_contract(&admin);
        client.record_ballot(&admin, &ballot_hash);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_only_admin_can_pause_contract() {
        let (env, client, _admin) = setup();
        let attacker = Address::generate(&env);

        client.pause_contract(&attacker);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_only_admin_can_resume_contract() {
        let (env, client, admin) = setup();
        let attacker = Address::generate(&env);

        client.pause_contract(&admin);
        client.resume_contract(&attacker);
    }
}
