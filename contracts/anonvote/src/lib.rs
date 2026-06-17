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
//! # Access control model (Issue #31)
//! - `Admin`          — full privileges; manages roles; immutable supremacy
//! - `BallotOperator` — can record ballots, tokens, and votes
//! - `ResultVerifier` — can publish results
//! - `ReadOnly`       — view-only; no state-changing calls permitted
//!
//! One address can hold multiple roles.  Roles are revoked immediately upon
//! `revoke_role`.  Admin always retains full privileges regardless of explicit
//! role grants.
//!
//! # Privacy guarantees
//! - No voter identifiers stored
//! - No token values stored
//! - No vote content stored
//! - Only counts and hashes — same privacy model as the off-chain system

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Env, String, Vec,
};

// ── Access-control roles ───────────────────────────────────────────────────────

/// Role variants that can be granted to / revoked from an address.
///
/// `Admin` itself is stored separately (see `DataKey::Admin`).  These variants
/// represent the delegated-role layer that sits below admin.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum Role {
    /// Can record ballots, issue tokens, and record votes.
    BallotOperator,
    /// Can publish / record results.
    ResultVerifier,
    /// Explicitly marks an address as read-only (no state mutations).
    ReadOnly,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address — only admin can manage roles
    Admin,
    /// Role list for an address: address → Vec<Role>
    Roles(Address),
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
    // ── Initialisation ───────────────────────────────────────────────────────

    /// Initialize the contract with an admin address.
    /// Must be called once after deployment.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    // ── Role management (admin only) ─────────────────────────────────────────

    /// Grant `role` to `grantee`.  Only the admin may call this.
    ///
    /// If the grantee already holds the role the call is a no-op (idempotent).
    pub fn grant_role(env: Env, caller: Address, grantee: Address, role: Role) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Roles(grantee.clone());
        let mut roles: Vec<Role> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| vec![&env]);

        // Idempotent: only push if not already present
        let mut already_has = false;
        for r in roles.iter() {
            if r == role {
                already_has = true;
                break;
            }
        }
        if !already_has {
            roles.push_back(role);
            env.storage().persistent().set(&key, &roles);
        }

        env.events()
            .publish((symbol_short!("role"),), (symbol_short!("granted"),));
    }

    /// Revoke `role` from `grantee`.  Only the admin may call this.
    ///
    /// Revocation takes effect immediately — subsequent calls by `grantee`
    /// that require the revoked role will be rejected.
    pub fn revoke_role(env: Env, caller: Address, grantee: Address, role: Role) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Roles(grantee.clone());
        let roles: Vec<Role> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| vec![&env]);

        // Rebuild without the revoked role
        let mut updated: Vec<Role> = vec![&env];
        for r in roles.iter() {
            if r != role {
                updated.push_back(r);
            }
        }
        env.storage().persistent().set(&key, &updated);

        env.events()
            .publish((symbol_short!("role"),), (symbol_short!("revoked"),));
    }

    /// Returns true if `addr` holds `role` (or is the admin).
    pub fn has_role(env: Env, addr: Address, role: Role) -> bool {
        // Admin always has every role
        if Self::is_admin(&env, &addr) {
            return true;
        }
        let key = DataKey::Roles(addr);
        let roles: Vec<Role> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| vec![&env]);
        for r in roles.iter() {
            if r == role {
                return true;
            }
        }
        false
    }

    // ── Write operations ─────────────────────────────────────────────────────

    /// Record a ballot creation event.
    /// Requires: `Admin` or `BallotOperator` role.
    /// ballot_id_hash: SHA-256 hex of the ballot UUID
    pub fn record_ballot(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::BallotOperator);

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
    /// Requires: `Admin` or `BallotOperator` role.
    pub fn record_token(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::BallotOperator);
        Self::require_ballot_exists(&env, &ballot_id_hash);

        let key = DataKey::TokensIssued(ballot_id_hash);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));

        env.events()
            .publish((symbol_short!("token"),), (symbol_short!("issued"),));
    }

    /// Increment the votes cast count for a ballot.
    /// Requires: `Admin` or `BallotOperator` role.
    pub fn record_vote(env: Env, caller: Address, ballot_id_hash: String) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::BallotOperator);
        Self::require_ballot_exists(&env, &ballot_id_hash);

        let key = DataKey::VotesCast(ballot_id_hash);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(count + 1));

        env.events()
            .publish((symbol_short!("vote"),), (symbol_short!("cast"),));
    }

    /// Record the result publication for a ballot.
    /// Requires: `Admin` or `ResultVerifier` role.
    /// result_hash: SHA-256 hex of the tally JSON
    pub fn record_result(
        env: Env,
        caller: Address,
        ballot_id_hash: String,
        result_hash: String,
    ) {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::ResultVerifier);
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

    /// Returns true if `addr` is the stored admin.
    fn is_admin(env: &Env, addr: &Address) -> bool {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        *addr == admin
    }

    /// Panics with `"unauthorized"` if `caller` is not the admin.
    fn require_admin(env: &Env, caller: &Address) {
        if !Self::is_admin(env, caller) {
            panic!("unauthorized");
        }
    }

    /// Panics with `"unauthorized"` unless `caller` holds `role` or is admin.
    ///
    /// Also rejects callers whose *only* role is `ReadOnly` — they may never
    /// invoke state-mutating methods even if they somehow passed the role check.
    fn require_role(env: &Env, caller: &Address, role: Role) {
        // Admin always passes
        if Self::is_admin(env, caller) {
            return;
        }

        // ReadOnly addresses are never allowed to mutate state
        let key = DataKey::Roles(caller.clone());
        let roles: Vec<Role> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| vec![env]);

        let mut has_required = false;
        let mut is_read_only_only = true;

        for r in roles.iter() {
            if r == role {
                has_required = true;
            }
            if r != Role::ReadOnly {
                is_read_only_only = false;
            }
        }

        if roles.is_empty() || !has_required || is_read_only_only {
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

    // ── Existing behaviour (regression) ─────────────────────────────────────

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

    // ── Role grant / revoke ──────────────────────────────────────────────────

    #[test]
    fn test_grant_ballot_operator_can_record() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "op_ballot");

        client.grant_role(&admin, &operator, &Role::BallotOperator);
        assert!(client.has_role(&operator, &Role::BallotOperator));

        client.record_ballot(&operator, &ballot_hash);
        client.record_token(&operator, &ballot_hash);
        client.record_vote(&operator, &ballot_hash);
        assert_eq!(client.get_tokens_issued(&ballot_hash), 1);
        assert_eq!(client.get_votes_cast(&ballot_hash), 1);
    }

    #[test]
    fn test_grant_result_verifier_can_publish() {
        let (env, client, admin) = setup();
        let verifier = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "v_ballot");
        let result_hash = String::from_str(&env, "cafebabe");

        // Admin creates the ballot first
        client.record_ballot(&admin, &ballot_hash);

        client.grant_role(&admin, &verifier, &Role::ResultVerifier);
        assert!(client.has_role(&verifier, &Role::ResultVerifier));

        client.record_result(&verifier, &ballot_hash, &result_hash);
        assert_eq!(client.get_result_hash(&ballot_hash), Some(result_hash));
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_result_verifier_cannot_record_ballot() {
        let (env, client, admin) = setup();
        let verifier = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "bad_ballot");

        client.grant_role(&admin, &verifier, &Role::ResultVerifier);
        // ResultVerifier must NOT be able to record a ballot
        client.record_ballot(&verifier, &ballot_hash);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_ballot_operator_cannot_publish_result() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "op_ballot2");
        let result_hash = String::from_str(&env, "aabbccdd");

        client.grant_role(&admin, &operator, &Role::BallotOperator);
        client.record_ballot(&operator, &ballot_hash);
        // BallotOperator must NOT be able to publish results
        client.record_result(&operator, &ballot_hash, &result_hash);
    }

    #[test]
    fn test_revoke_role_denies_access() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "revoke_ballot");

        client.grant_role(&admin, &operator, &Role::BallotOperator);
        client.record_ballot(&operator, &ballot_hash);

        // Revoke and confirm access is removed immediately
        client.revoke_role(&admin, &operator, &Role::BallotOperator);
        assert!(!client.has_role(&operator, &Role::BallotOperator));
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_revoked_operator_cannot_record_token() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "revoke_token_ballot");

        client.grant_role(&admin, &operator, &Role::BallotOperator);
        client.record_ballot(&operator, &ballot_hash);
        client.revoke_role(&admin, &operator, &Role::BallotOperator);

        // Must panic after revoke
        client.record_token(&operator, &ballot_hash);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_read_only_cannot_mutate() {
        let (env, client, admin) = setup();
        let reader = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "ro_ballot");

        client.grant_role(&admin, &reader, &Role::ReadOnly);
        // ReadOnly must never mutate state
        client.record_ballot(&reader, &ballot_hash);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_unknown_address_denied() {
        let (env, client, _admin) = setup();
        let attacker = Address::generate(&env);
        let ballot_hash = String::from_str(&env, "atk_ballot");
        client.record_ballot(&attacker, &ballot_hash);
    }

    #[test]
    fn test_address_can_hold_multiple_roles() {
        let (env, client, admin) = setup();
        let multi = Address::generate(&env);

        client.grant_role(&admin, &multi, &Role::BallotOperator);
        client.grant_role(&admin, &multi, &Role::ResultVerifier);

        assert!(client.has_role(&multi, &Role::BallotOperator));
        assert!(client.has_role(&multi, &Role::ResultVerifier));

        let ballot_hash = String::from_str(&env, "multi_ballot");
        let result_hash = String::from_str(&env, "multihash");
        client.record_ballot(&multi, &ballot_hash);
        client.record_result(&multi, &ballot_hash, &result_hash);
    }

    #[test]
    fn test_grant_is_idempotent() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);

        // Double-grant must not error
        client.grant_role(&admin, &operator, &Role::BallotOperator);
        client.grant_role(&admin, &operator, &Role::BallotOperator);
        assert!(client.has_role(&operator, &Role::BallotOperator));
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_non_admin_cannot_grant_role() {
        let (env, client, _admin) = setup();
        let rogue = Address::generate(&env);
        let target = Address::generate(&env);
        // Non-admin must not be able to grant roles
        client.grant_role(&rogue, &target, &Role::BallotOperator);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_non_admin_cannot_revoke_role() {
        let (env, client, admin) = setup();
        let operator = Address::generate(&env);
        let rogue = Address::generate(&env);

        client.grant_role(&admin, &operator, &Role::BallotOperator);
        // Non-admin must not be able to revoke
        client.revoke_role(&rogue, &operator, &Role::BallotOperator);
    }

    #[test]
    fn test_admin_always_has_full_access() {
        let (_env, client, admin) = setup();
        assert!(client.has_role(&admin, &Role::BallotOperator));
        assert!(client.has_role(&admin, &Role::ResultVerifier));
        assert!(client.has_role(&admin, &Role::ReadOnly));
    }
}
