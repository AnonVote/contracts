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

## Deploy

### Prerequisites

- [Rust](https://rustup.rs/) with the `wasm32-unknown-unknown` target
- [Stellar CLI](https://github.com/stellar/stellar-cli)
- `jq` installed
- A Stellar account funded with testnet/mainnet XLM

### Environment variables

Copy `.env.example` to `.env` and fill in:

```bash
cp .env.example .env
```

| Variable | Required | Description |
|---|---|---|
| `STELLAR_SECRET_KEY` | Yes | Admin secret key for signing the deploy transaction |
| `SOROBAN_RPC_URL` | No | RPC endpoint — defaults are set per network in `deploy.sh` |

### Deploy to testnet

```bash
source .env
./deploy.sh testnet
```

### Deploy to mainnet

```bash
source .env
./deploy.sh mainnet
```

### What the script does

1. Builds the WASM binary
2. Deploys to the specified Stellar network
3. Initializes the contract with the derived admin address
4. Records contract ID, WASM hash, git commit, and timestamp in `deployments.json`
5. Creates a git tag (`contract-testnet-v1.0.0` or `contract-mainnet-v1.0.0`)
6. Prints a summary with the contract ID and verification links

### Verifying deployment

Check the contract on Stellar Explorer:
- Testnet: `https://stellar.expert/explorer/testnet/contract/<CONTRACT_ID>`
- Mainnet: `https://stellar.expert/explorer/mainnet/contract/<CONTRACT_ID>`

### Pushing to remote

After deployment, push the tag and commit:

```bash
git push origin contract-testnet-v1.0.0
git push origin feat/contract-deployment-script
```

---

## Wire into the backend

Once deployed, update `backend/src/services/sorobanService.ts` calls in:

- `backend/src/services/ballotEngine.ts` — call `invokeContract(id, "record_ballot", [...])`
- `backend/src/services/identityManager.ts` — call `invokeContract(id, "record_token", [...])`
- `backend/src/services/privacyEngine.ts` — call `invokeContract(id, "record_vote", [...])`
- `backend/src/services/resultEngine.ts` — call `invokeContract(id, "record_result", [...])`

The `ballot_id_hash` argument should be `hashIdentifier(ballotId)` — the same SHA-256 function already used in the backend.

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
