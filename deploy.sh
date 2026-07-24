#!/usr/bin/env bash
set -euo pipefail

NETWORK="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACT_DIR="$SCRIPT_DIR/anonvote"
WASM_PATH="$CONTRACT_DIR/target/wasm32-unknown-unknown/release/anonvote.wasm"
DEPLOYMENTS_FILE="$SCRIPT_DIR/deployments.json"
CONTRACT_NAME="anonvote"

usage() {
  echo "Usage: $0 <testnet|mainnet>"
  echo ""
  echo "Environment variables:"
  echo "  STELLAR_SECRET_KEY   Admin secret key for signing"
  echo "  SOROBAN_RPC_URL      Stellar RPC endpoint (default per network)"
  exit 1
}

if [[ -z "$NETWORK" ]]; then
  usage
fi

if [[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]]; then
  echo "Error: network must be 'testnet' or 'mainnet'"
  usage
fi

# ---------- prerequisite checks ----------
for cmd in cargo stellar jq git; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "Error: $cmd is not installed."
    exit 1
  fi
done

if [[ -z "${STELLAR_SECRET_KEY:-}" ]]; then
  echo "Error: STELLAR_SECRET_KEY is not set."
  exit 1
fi

# ---------- network config ----------
case "$NETWORK" in
  testnet)
    SOROBAN_RPC_URL="${SOROBAN_RPC_URL:-https://soroban-testnet.stellar.org}"
    SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 15"
    ;;
  mainnet)
    SOROBAN_RPC_URL="${SOROBAN_RPC_URL:-https://soroban-mainnet.stellar.org}"
    SOROBAN_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
    ;;
esac

echo "=== AnonVote Contract Deployment ==="
echo "Network:     $NETWORK"
echo "RPC URL:     $SOROBAN_RPC_URL"
echo "WASM:        $WASM_PATH"
echo "Deployments: $DEPLOYMENTS_FILE"
echo ""

# ---------- 1. Build ----------
echo ">>> Building contract..."
cargo build --manifest-path "$CONTRACT_DIR/Cargo.toml" \
  --target wasm32-unknown-unknown --release

if [[ ! -f "$WASM_PATH" ]]; then
  echo "Error: WASM not found at $WASM_PATH"
  exit 1
fi
echo "    Build complete."
echo ""

# ---------- 2. Compute hashes ----------
WASM_HASH=$(sha256sum "$WASM_PATH" | awk '{print $1}')
GIT_COMMIT=$(git -C "$SCRIPT_DIR" rev-parse --short HEAD 2>/dev/null || echo "unknown")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
echo "    WASM SHA-256: $WASM_HASH"
echo "    Git commit:   $GIT_COMMIT"
echo ""

# ---------- 3. Deploy ----------
echo ">>> Deploying $CONTRACT_NAME to $NETWORK..."
DEPLOY_OUTPUT=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$STELLAR_SECRET_KEY" \
  --network "$NETWORK" \
  --rpc-url "$SOROBAN_RPC_URL" \
  --network-passphrase "$SOROBAN_NETWORK_PASSPHRASE" \
  2>&1)

# stellar contract deploy prints the contract ID on success
CONTRACT_ID=$(echo "$DEPLOY_OUTPUT" | grep -oE 'C[A-Z0-9]{55}' | head -1)

if [[ -z "$CONTRACT_ID" ]]; then
  echo "Error: failed to extract contract ID from deployment output."
  echo "Output: $DEPLOY_OUTPUT"
  exit 1
fi

echo "    Contract ID: $CONTRACT_ID"
echo ""

# ---------- 4. Initialize contract ----------
echo ">>> Initializing contract..."
ADMIN_ADDRESS=$(echo "$STELLAR_SECRET_KEY" | stellar keys address --stdin 2>/dev/null || true)
if [[ -z "$ADMIN_ADDRESS" ]]; then
  echo "    Warning: could not derive admin address from secret key. Skipping initialization."
  echo "    Run manually:"
  echo "      stellar contract invoke --id $CONTRACT_ID --source <ADMIN_SECRET> --network $NETWORK -- initialize --admin <ADMIN_ADDRESS>"
else
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source "$STELLAR_SECRET_KEY" \
    --network "$NETWORK" \
    --rpc-url "$SOROBAN_RPC_URL" \
    --network-passphrase "$SOROBAN_NETWORK_PASSPHRASE" \
    -- initialize \
    --admin "$ADMIN_ADDRESS"
  echo "    Initialized with admin: $ADMIN_ADDRESS"
fi
echo ""

# ---------- 5. Record deployment ----------
echo ">>> Recording deployment to $DEPLOYMENTS_FILE..."

# Seed file if it doesn't exist
if [[ ! -f "$DEPLOYMENTS_FILE" ]]; then
  echo '{}' > "$DEPLOYMENTS_FILE"
fi

# Build updated JSON with jq
TEMP_FILE=$(mktemp)
jq --arg net "$NETWORK" \
   --arg cid "$CONTRACT_ID" \
   --arg wasm "$WASM_HASH" \
   --arg commit "$GIT_COMMIT" \
   --arg ts "$TIMESTAMP" \
   --arg rpc "$SOROBAN_RPC_URL" \
   '.[$net] = {
      "contract_id": $cid,
      "transaction_id": null,
      "timestamp": $ts,
      "wasm_hash": $wasm,
      "git_commit": $commit,
      "rpc_url": $rpc
    }' "$DEPLOYMENTS_FILE" > "$TEMP_FILE"
mv "$TEMP_FILE" "$DEPLOYMENTS_FILE"

echo "    Saved."
echo ""

# ---------- 6. Git tag ----------
TAG="contract-${NETWORK}-v1.0.0"
echo ">>> Creating git tag: $TAG"
git -C "$SCRIPT_DIR" add "$DEPLOYMENTS_FILE"
if git -C "$SCRIPT_DIR" diff --cached --quiet; then
  echo "    No changes to commit."
else
  git -C "$SCRIPT_DIR" commit -m "deploy: $CONTRACT_NAME to $NETWORK ($CONTRACT_ID)"
fi
git -C "$SCRIPT_DIR" tag -f "$TAG" -m "Deploy $CONTRACT_NAME to $NETWORK: $CONTRACT_ID" 2>/dev/null || \
  git -C "$SCRIPT_DIR" tag -a "$TAG" -m "Deploy $CONTRACT_NAME to $NETWORK: $CONTRACT_ID"
echo ""

# ---------- 7. Summary ----------
echo "============================================"
echo "  Deployment complete!"
echo "  Network:      $NETWORK"
echo "  Contract ID:  $CONTRACT_ID"
echo "  WASM Hash:    $WASM_HASH"
echo "  Git Commit:   $GIT_COMMIT"
echo "  Git Tag:      $TAG"
echo "============================================"
echo ""
echo "Next steps:"
echo "  1. Verify on Stellar Explorer: https://stellar.expert/explorer/$NETWORK/contract/$CONTRACT_ID"
echo "  2. Add to backend .env: SOROBAN_CONTRACT_ID=$CONTRACT_ID"
echo "  3. Push tag:    git push origin $TAG"
echo "  4. Push commit: git push origin $NETWORK"
