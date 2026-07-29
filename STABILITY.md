# Stability Guarantees

## ContractError Discriminant Values

The `ContractError` enum defines error codes returned by the AnonVote Soroban
contract. Each variant has a fixed `#[repr(u32)]` discriminant.

**These discriminant values must never change after the contract is deployed.**
Consumers that parse error codes from Stellar transaction results depend on the
numeric value, not the variant name. Changing a value is a breaking change.

| Variant                  | Value | Description                                    |
|--------------------------|-------|------------------------------------------------|
| AdminUnauthorized        | 1     | caller is not the admin                        |
| AlreadyInitialized       | 2     | initialize called after contract is set up     |
| NotInitialized           | 3     | write operation before initialize              |
| BallotNotFound           | 4     | write op on unregistered ballot                |
| BallotAlreadyExists      | 5     | record_ballot with existing ballot hash        |
| ResultAlreadyPublished   | 6     | different result hash already published        |
| CounterOverflow          | 7     | token/vote counter exceeds u32::MAX            |
| InvalidBallotHash        | 8     | ballot_id_hash is empty                        |
| UpgradeAlreadyScheduled  | 9     | upgrade already pending                        |
| NoUpgradeScheduled       | 10    | upgrade requested but none scheduled           |
| TimeLockNotExpired       | 11    | upgrade time lock still active                 |
| BallotExpired            | 12    | ballot has been expired                        |
| ContractPaused           | 13    | write operation while contract is paused       |
| LimitExceeded            | 14    | token/vote count hits ballot limit             |
| InvalidApprovalConfig    | 15    | M-of-N config is invalid                       |
| DuplicateApprover        | 16    | duplicate address in approver list             |
| ApproverUnauthorized     | 17    | address is not a configured approver           |
| OperationNotFound        | 18    | operation_id does not exist                    |
| OperationAlreadyApproved | 19    | approver already approved this operation       |
| OperationNotPending       | 20    | operation is not in Pending status             |
| OperationExpired         | 21    | approval window has passed                     |
| SameAdmin                | 22    | rotation target equals current admin           |
| InvalidBallotIdHash      | 23    | ballot_id_hash is not valid 64-char hex        |
| InvalidResultHash        | 24    | result_hash is not valid 64-char hex           |
| InvalidAdminAddress      | 25    | initialize/rotate target is zero address       |

## Storage Key Stability

Storage keys are derived from the `DataKey` enum variants. Variant names and
their associated tuple types must not be changed or reordered after deployment.

| Variant              | Type              | Location   |
|----------------------|-------------------|------------|
| Admin                | `Address`         | instance   |
| InitializedAt        | `u64`             | instance   |
| IsPaused             | `bool`            | instance   |
| Approvers            | `Vec<Address>`    | instance   |
| ApprovalThreshold    | `u32`             | instance   |
| OperationNonce       | `u64`             | instance   |
| Operation(id)        | `PendingOperation`| persistent |
| Approval(id, addr)   | `bool`            | persistent |
| OperationApprover(id, addr) | `bool`    | persistent |
| TokensIssued(hash)   | `u32`             | persistent |
| VotesCast(hash)      | `u32`             | persistent |
| ResultHash(hash)     | `String`          | persistent |
| BallotMetadata(hash) | `BallotMetadata`  | persistent |
| BallotExpired(hash)  | `bool`            | persistent |
| PendingUpgrade       | `PendingUpgrade`  | instance   |
| RotationHistory      | `Vec<RotationRecord>` | persistent |

## Event Topics

Events published by the contract use fixed two-symbol topics. Adding new topics
is safe; changing or removing existing ones is breaking for off-chain indexers.

| Topic                         | Emitted By                   |
|-------------------------------|------------------------------|
| `("govern", "cfg_appr")`      | configure_approval_threshold |
| `("govern", "op_create")`     | create_operation             |
| `("govern", "approved")`      | approve_operation            |
| `("govern", "op_exec")`       | approve_operation (M-of-N reached) |
| `("govern", "op_cancel")`     | cancel_operation             |
| `("audit", "blt_crtd")`       | record_ballot, record_ballots_batch |
| `("audit", "tok_issd")`       | record_token                 |
| `("audit", "vote_cast")`      | record_vote                  |
| `("audit", "res_pub")`        | execute_operation (result)   |
| `("audit", "exp_adm")`        | expire_ballot                |
| `("audit", "paused")`         | execute_operation (pause)    |
| `("audit", "upg_schd")`       | execute_operation (upgrade)  |
| `("audit", "upg_cncl")`       | cancel_upgrade               |
| `("audit", "upg_excd")`       | execute_upgrade              |
| `("audit", "resumed")`        | resume_contract              |
| `("admin", "rotated")`        | execute_operation (rotation) |
| `("ballot",)` (with `BallotEvent` payload) | record_ballot, record_token, record_vote, execute_operation |
| `("init", "invalid")`         | verify_initialized           |
| `("counter", "ovrflw")`       | record_token / record_vote on u32::MAX overflow |
