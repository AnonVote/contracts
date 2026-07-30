# Stability Guarantees

## ContractError Discriminant Values

The `ContractError` enum in `contracts/anonvote/src/errors.rs` defines error codes
returned by the AnonVote Soroban contract. Each variant has a fixed `#[repr(u32)]`
discriminant.

**These discriminant values must never change after the contract is deployed.**
Consumers that parse error codes from Stellar transaction results depend on the
numeric value, not the variant name. Changing a value is a breaking change.

| Variant                | Value | Description                              |
|------------------------|-------|------------------------------------------|
| AlreadyInitialized     | 1     | initialize called after admin is set     |
| Unauthorized           | 2     | caller is not the admin                  |
| BallotAlreadyExists    | 3     | record_ballot called with existing hash  |
| BallotNotFound         | 4     | write op with unregistered ballot        |
| BallotAlreadyFinalised | 5     | record_result after result is set        |
| InvalidBallotIdHash    | 6     | ballot_id_hash not valid 64-char hex     |
| InvalidResultHash      | 7     | result_hash not valid 64-char hex        |
| InvalidAdminAddress    | 8     | new admin address is zero or same        |
| CounterOverflow        | 9     | token/vote counter exceeds u32::MAX      |
| BallotExpired          | 10    | operation after ballot ledger expiry     |

## Storage Key Stability

Storage keys used for instance and persistent storage are derived from the
`DataKey` enum variants. Variant names must not be changed or reordered after
deployment.

## Event Topics

Events published by the contract use fixed topic symbols. Changing topic
symbols is a breaking change for off-chain event indexers.
