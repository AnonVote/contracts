# AnonVote Soroban Smart Contract

Records immutable audit events on the Stellar blockchain with on-chain queryable state.

## What it does

| Function                                     | Description                  |
| -------------------------------------------- | ---------------------------- |
| `record_ballot(ballot_id_hash)`              | Register a ballot on-chain   |
| `record_token(ballot_id_hash)`               | Increment token issued count |
| `record_vote(ballot_id_hash)`                | Increment vote cast count    |
| `record_result(ballot_id_hash, result_hash)` | Publish result hash          |
| `get_tokens_issued(ballot_id_hash)`          | Read token count             |
| `get_votes_cast(ballot_id_hash)`             | Read vote count              |
| `get_result_hash(ballot_id_hash)`            | Read result hash             |
| `is_consistent(ballot_id_hash)`              | Check tokens == votes        |

All inputs use SHA-256 hashes of ballot UUIDs — no raw IDs stored on-chain.

---

## Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Stellar CLI
cargo install --locked stellar-cli --features opt
```

---

## Build

```bash
cd contracts/anonvote
cargo build --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/anonvote.wasm`

---

## Test

```bash
cd contracts/anonvote
cargo test
```

---

## Deploy to Testnet

```bash
# Deploy the contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/anonvote.wasm \
  --source SBQE4MLYZGNOQOXHTHHKGR6L6YZ72HTRY6TDDMKS5FMESKPG27O4HK7K \
  --network testnet

# Output: CONTRACT_ID (e.g. CABC123...)
# Add to backend/.env:
# SOROBAN_CONTRACT_ID=CABC123...
```

---

## Initialize after deployment

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source SBQE4MLYZGNOQOXHTHHKGR6L6YZ72HTRY6TDDMKS5FMESKPG27O4HK7K \
  --network testnet \
  -- initialize \
  --admin GCSL4DBZNKTBNA4FGWFSCWFC4GDCMYAUKD6BISRJHIU4V4K5WVNYOLBN
```

---

## Wire into the backend

Once deployed, update `backend/src/services/sorobanService.ts` calls in:

- `backend/src/services/ballotEngine.ts` — call `invokeContract(id, "record_ballot", [...])`
- `backend/src/services/identityManager.ts` — call `invokeContract(id, "record_token", [...])`
- `backend/src/services/privacyEngine.ts` — call `invokeContract(id, "record_vote", [...])`
- `backend/src/services/resultEngine.ts` — call `invokeContract(id, "record_result", [...])`

The `ballot_id_hash` argument should be `hashIdentifier(ballotId)` — the same SHA-256 function already used in the backend.

All five service helpers (`sorobanRecordBallot`, `sorobanRecordBallotsBatch`, `sorobanRecordToken`, `sorobanRecordVote`, `sorobanRecordResult`) throw `SorobanServiceError` on any failure. Import it from `service/index.ts` and wrap every call:

```ts
import { SorobanServiceError } from "@anonvote/contracts/service";

try {
  await sorobanRecordBallot(config, ballotIdHash);
} catch (err) {
  if (err instanceof SorobanServiceError) {
    if (err.retryable) {
      // Enqueue for retry with backoff — NETWORK_ERROR and SIMULATION_FAILED
      // are transient; retrying is safe and expected.
    } else {
      // CONTRACT_ERROR or TRANSACTION_FAILED — do not retry.
      // CONTRACT_ERROR indicates a logic error (e.g. ballot already exists).
      // Retrying it will always produce the same result.
    }
  }
  throw err;
}
```

Error code retryability:

| `SorobanServiceErrorCode` | `retryable` | When thrown                                   |
| -------------------------- | ----------- | --------------------------------------------- |
| `NETWORK_ERROR`            | `true`      | RPC endpoint unreachable, DNS failure, TCP reset |
| `SIMULATION_FAILED`        | `true`      | RPC timeout, overloaded node, no error code   |
| `TRANSACTION_FAILED`       | `false`     | `sendTransaction` returned ERROR, or tx never confirmed after max retries |
| `CONTRACT_ERROR`           | `false`     | On-chain logic error (BallotNotFound, BallotAlreadyExists, etc.) |

Full error details (raw RPC responses, contract diagnostics) are logged internally only and are never exposed in the thrown error message, so it is safe to surface `err.message` in structured logs. Do not include raw contract error details in API responses to clients.

---

## Milestones

AnonVote development is organized into three milestones. Each issue is tagged with which milestone it belongs to.

### Milestone 1 — Foundation

Everything works end-to-end on testnet. A real admin can create a ballot, upload voters, issue tokens, collect votes, tally, and verify the result on Stellar. No manual database steps.

**Status:** In progress
**Focus:** Core voting flow, Soroban integration, vote encryption, public verification

### Milestone 2 — Hardening

The system is production-safe. Per-ballot encryption keys, rate limiting, error handling, retry queues, no raw identifiers anywhere, Soroban fully wired not stubbed.

**Status:** Planned
**Focus:** Security hardening, production readiness, reliability, scalability

### Milestone 3 — Ecosystem

@anonvote/crypto published on npm, docs repo complete, contracts deployed on mainnet, third party developers can build on top of AnonVote using the JS SDK.

**Status:** Planned
**Focus:** SDK release, third-party integrations, documentation

---

## Contributing

Issues are labeled with their corresponding milestone so you can see what stage of development they belong to.
