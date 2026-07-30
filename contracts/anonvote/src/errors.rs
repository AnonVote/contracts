use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized     = 1,
    Unauthorized           = 2,
    BallotAlreadyExists    = 3,
    BallotNotFound         = 4,
    BallotAlreadyFinalised = 5,
    InvalidBallotIdHash    = 6,
    InvalidResultHash      = 7,
    InvalidAdminAddress    = 8,
    CounterOverflow        = 9,
    BallotExpired          = 10,
}
