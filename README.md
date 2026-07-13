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
