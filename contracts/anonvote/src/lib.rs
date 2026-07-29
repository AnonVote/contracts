#![no_std]

mod errors;
mod validation;

pub use errors::ContractError;

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};
use validation::validate_hex_hash;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BallotState {
    Active,
    ResultPublished,
}

#[contracttype]
#[derive(Clone)]
pub struct BallotMetadata {
    pub created_at: u64,
    pub admin: Address,
    pub state: BallotState,
}

#[contracttype]
#[derive(Clone)]
pub struct BallotStateSnapshot {
    pub tokens_issued: u32,
    pub votes_cast: u32,
    pub result_hash: Option<String>,
    pub created_at: u64,
    pub admin: Address,
    pub state: BallotState,
}

#[contracttype]
#[derive(Clone)]
pub struct AdminRotation {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Initialized,
    InitializedAt,
    AdminHistory,
    TokensIssued(String),
    VotesCast(String),
    ResultHash(String),
    BallotExists(String),
    BallotMetadata(String),
}

#[contract]
pub struct AnonVoteContract;

#[contractimpl]
impl AnonVoteContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ContractError::AlreadyInitialized);
        }

        let zero_key =
            String::from_str(&env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let zero = Address::from_string(&zero_key);
        if admin == zero {
            return Err(ContractError::InvalidAdminAddress);
        }

        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::InitializedAt, &env.ledger().timestamp());
        Ok(())
    }

    pub fn record_ballot(
        env: Env,
        caller: Address,
        ballot_id_hash: String,
    ) -> Result<(), ContractError> {
        validate_hex_hash(&env, &ballot_id_hash, ContractError::InvalidBallotIdHash)?;
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let exists_key = DataKey::BallotExists(ballot_id_hash.clone());
        if env.storage().persistent().has(&exists_key) {
            return Err(ContractError::BallotAlreadyExists);
        }

        env.storage().persistent().set(&exists_key, &true);
        env.storage()
            .persistent()
            .set(&DataKey::TokensIssued(ballot_id_hash.clone()), &0u32);
        env.storage()
            .persistent()
            .set(&DataKey::VotesCast(ballot_id_hash.clone()), &0u32);

        let created_at = env.ledger().timestamp();
        let metadata = BallotMetadata {
            created_at,
            admin: caller.clone(),
            state: BallotState::Active,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BallotMetadata(ballot_id_hash.clone()), &metadata);

        env.events().publish(
            (symbol_short!("ballot"), symbol_short!("regstrd")),
            (ballot_id_hash, created_at),
        );
        Ok(())
    }

    pub fn record_token(
        env: Env,
        caller: Address,
        ballot_id_hash: String,
    ) -> Result<(), ContractError> {
        validate_hex_hash(&env, &ballot_id_hash, ContractError::InvalidBallotIdHash)?;
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        Self::require_ballot_exists(&env, &ballot_id_hash)?;
        Self::require_not_finalised(&env, &ballot_id_hash)?;

        let key = DataKey::TokensIssued(ballot_id_hash.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if count == u32::MAX {
            env.events().publish(
                (symbol_short!("counter"), symbol_short!("ovrflw")),
                (ballot_id_hash, count),
            );
            return Err(ContractError::CounterOverflow);
        }
        let new_count = count + 1;
        env.storage().persistent().set(&key, &new_count);

        env.events().publish(
            (symbol_short!("ballot"), symbol_short!("tok_issd")),
            (ballot_id_hash, new_count),
        );
        Ok(())
    }

    pub fn record_vote(
        env: Env,
        caller: Address,
        ballot_id_hash: String,
    ) -> Result<(), ContractError> {
        validate_hex_hash(&env, &ballot_id_hash, ContractError::InvalidBallotIdHash)?;
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        Self::require_ballot_exists(&env, &ballot_id_hash)?;
        Self::require_not_finalised(&env, &ballot_id_hash)?;

        let key = DataKey::VotesCast(ballot_id_hash.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        if count == u32::MAX {
            env.events().publish(
                (symbol_short!("counter"), symbol_short!("ovrflw")),
                (ballot_id_hash, count),
            );
            return Err(ContractError::CounterOverflow);
        }
        let new_count = count + 1;
        env.storage().persistent().set(&key, &new_count);

        env.events().publish(
            (symbol_short!("ballot"), symbol_short!("vote_cast")),
            (ballot_id_hash, new_count),
        );
        Ok(())
    }

    pub fn record_result(
        env: Env,
        caller: Address,
        ballot_id_hash: String,
        result_hash: String,
    ) -> Result<(), ContractError> {
        validate_hex_hash(&env, &result_hash, ContractError::InvalidResultHash)?;
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        Self::require_ballot_exists(&env, &ballot_id_hash)?;
        Self::require_not_finalised(&env, &ballot_id_hash)?;

        let key = DataKey::ResultHash(ballot_id_hash.clone());
        env.storage().persistent().set(&key, &result_hash.clone());

        let metadata_key = DataKey::BallotMetadata(ballot_id_hash.clone());
        let mut metadata: BallotMetadata =
            env.storage().persistent().get(&metadata_key).unwrap();
        metadata.state = BallotState::ResultPublished;
        env.storage().persistent().set(&metadata_key, &metadata);

        env.events().publish(
            (symbol_short!("result"), symbol_short!("pubshd")),
            (ballot_id_hash, result_hash, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn rotate_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let zero_key =
            String::from_str(&env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let zero = Address::from_string(&zero_key);
        if new_admin == zero {
            return Err(ContractError::InvalidAdminAddress);
        }
        let current: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if new_admin == current {
            return Err(ContractError::InvalidAdminAddress);
        }

        let timestamp = env.ledger().timestamp();
        let rotation = AdminRotation {
            old_admin: current.clone(),
            new_admin: new_admin.clone(),
            timestamp,
        };

        let mut history: Vec<AdminRotation> = env
            .storage()
            .instance()
            .get(&DataKey::AdminHistory)
            .unwrap_or(Vec::new(&env));
        history.push_back(rotation);
        env.storage()
            .instance()
            .set(&DataKey::AdminHistory, &history);
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (symbol_short!("admin"), symbol_short!("rotated")),
            (caller, new_admin, timestamp),
        );
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn get_admin_history(env: Env) -> Vec<AdminRotation> {
        env.storage()
            .instance()
            .get(&DataKey::AdminHistory)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_tokens_issued(env: Env, ballot_id_hash: String) -> Option<u32> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            return None;
        }
        env.storage()
            .persistent()
            .get(&DataKey::TokensIssued(ballot_id_hash))
    }

    pub fn get_votes_cast(env: Env, ballot_id_hash: String) -> Option<u32> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            return None;
        }
        env.storage()
            .persistent()
            .get(&DataKey::VotesCast(ballot_id_hash))
    }

    pub fn get_result_hash(env: Env, ballot_id_hash: String) -> Option<String> {
        env.storage()
            .persistent()
            .get(&DataKey::ResultHash(ballot_id_hash))
    }

    pub fn ballot_exists(env: Env, ballot_id_hash: String) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash))
    }

    pub fn result_exists(env: Env, ballot_id_hash: String) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::ResultHash(ballot_id_hash))
    }

    pub fn get_initialized_at(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::InitializedAt)
    }

    pub fn get_ballot_metadata(env: Env, ballot_id_hash: String) -> Option<BallotMetadata> {
        env.storage()
            .persistent()
            .get(&DataKey::BallotMetadata(ballot_id_hash))
    }

    pub fn get_ballot_state(env: Env, ballot_id_hash: String) -> Option<BallotStateSnapshot> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            return None;
        }

        let tokens_issued: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TokensIssued(ballot_id_hash.clone()))
            .unwrap_or(0);
        let votes_cast: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::VotesCast(ballot_id_hash.clone()))
            .unwrap_or(0);
        let result_hash: Option<String> = env
            .storage()
            .persistent()
            .get(&DataKey::ResultHash(ballot_id_hash.clone()));
        let metadata: BallotMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::BallotMetadata(ballot_id_hash))
            .unwrap();

        Some(BallotStateSnapshot {
            tokens_issued,
            votes_cast,
            result_hash,
            created_at: metadata.created_at,
            admin: metadata.admin,
            state: metadata.state,
        })
    }

    pub fn is_consistent(env: Env, ballot_id_hash: String) -> bool {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            return false;
        }
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

    fn require_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::Unauthorized)?;
        if *caller != admin {
            return Err(ContractError::Unauthorized);
        }
        Ok(())
    }

    fn require_ballot_exists(env: &Env, ballot_id_hash: &String) -> Result<(), ContractError> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::BallotExists(ballot_id_hash.clone()))
        {
            return Err(ContractError::BallotNotFound);
        }
        Ok(())
    }

    fn require_not_finalised(env: &Env, ballot_id_hash: &String) -> Result<(), ContractError> {
        if env
            .storage()
            .persistent()
            .has(&DataKey::ResultHash(ballot_id_hash.clone()))
        {
            return Err(ContractError::BallotAlreadyFinalised);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
