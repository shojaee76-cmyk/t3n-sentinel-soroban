#!/usr/bin/env bash
# Deploy sentinel-vault + sentinel-oracle to the Soroban testnet.
#
# Prereqs (one-time):
#   cargo install --locked soroban-cli --features opt
#   soroban network add --rpc-url https://soroban-testnet.stellar.org:443 \
#     --network-passphrase "Test SDF Network ; September 2015" testnet
#   soroban keys generate alice
#
# Usage:
#   ./scripts/deploy_testnet.sh          # builds + deploys both contracts
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
SOURCE="${SOURCE:-alice}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> Building WASM (release-with-logs)"
cd "$ROOT"
cargo build --target wasm32-unknown-unknown --release --profile release-with-logs

WASMS=(
  "contracts/sentinel-vault/target/wasm32-unknown-unknown/release-with-logs/sentinel_vault.wasm"
  "contracts/sentinel-oracle/target/wasm32-unknown-unknown/release-with-logs/sentinel_oracle.wasm"
)
for wasm in "${WASMS[@]}"; do
  [ -f "$wasm" ] || { echo "missing $wasm — run cargo build first"; exit 1; }
done

echo "==> Deploying sentinel-vault"
VAULT_ID=$(soroban contract deploy \
  --network "$NETWORK" --source "$SOURCE" \
  --wasm "contracts/sentinel-vault/target/wasm32-unknown-unknown/release-with-logs/sentinel_vault.wasm")
echo "sentinel-vault: $VAULT_ID"

echo "==> Deploying sentinel-oracle"
ORACLE_ID=$(soroban contract deploy \
  --network "$NETWORK" --source "$SOURCE" \
  --wasm "contracts/sentinel-oracle/target/wasm32-unknown-unknown/release-with-logs/sentinel_oracle.wasm")
echo "sentinel-oracle: $ORACLE_ID"

echo
echo "Deployed:"
echo "  sentinel-vault  $VAULT_ID"
echo "  sentinel-oracle $ORACLE_ID"
echo
echo "Next: soroban contract invoke --id $VAULT_ID --network $NETWORK --source $SOURCE -- init \\
    --authority \$(soroban keys address $SOURCE) --tee_worker <TEE_WORKER_ADDR>"
