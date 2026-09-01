#!/bin/bash
# Deploy t3n-sentinel to Soroban testnet (stellar-cli 27.x)
# Usage: bash scripts/deploy_testnet.sh
# Requires: stellar-cli on PATH (stellar.exe), identity "sentinel" generated + funded
set -euo pipefail

STELLAR="stellar"
NET="--network testnet"
SRC="--source sentinel"
IDENT="GDFUVJ47JMNKUCPUJHITIZ6GFWEEQCZNNJHJOWGRPLXE3OS6NIZTLUWM"
NATIVE_XLM_SAC="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
WASM_DIR="target/wasm32v1-none/release"

# 0. Build wasm (wasm32v1-none is the soroban-sdk 27 target)
echo "=== building wasm ==="
rustup target add wasm32v1-none >/dev/null 2>&1 || true
cargo build --release --target wasm32v1-none -p sentinel-vault -p sentinel-oracle -p sentinel-payment -p sentinel-sac

# 1. Upload
echo "=== uploading wasm ==="
V_HASH=$($STELLAR contract upload --wasm "$WASM_DIR/sentinel_vault.wasm" $SRC $NET | tail -1)
O_HASH=$($STELLAR contract upload --wasm "$WASM_DIR/sentinel_oracle.wasm" $SRC $NET | tail -1)
P_HASH=$($STELLAR contract upload --wasm "$WASM_DIR/sentinel_payment.wasm" $SRC $NET | tail -1)
S_HASH=$($STELLAR contract upload --wasm "$WASM_DIR/sentinel_sac.wasm" $SRC $NET | tail -1)

# 2. Deploy
echo "=== deploying ==="
VAULT=$($STELLAR contract deploy --wasm-hash "$V_HASH" $SRC $NET | tail -1)
ORACLE=$($STELLAR contract deploy --wasm-hash "$O_HASH" $SRC $NET | tail -1)
PAYMENT=$($STELLAR contract deploy --wasm-hash "$P_HASH" $SRC $NET | tail -1)
SAC=$($STELLAR contract deploy --wasm-hash "$S_HASH" $SRC $NET | tail -1)
echo "VAULT=$VAULT ORACLE=$ORACLE PAYMENT=$PAYMENT SAC=$SAC"

# 3. Init
echo "=== init ==="
$STELLAR contract invoke --id "$VAULT" $SRC $NET -- init --authority "$IDENT" --tee_worker "$IDENT"
$STELLAR contract invoke --id "$ORACLE" $SRC $NET -- init --operator "$IDENT"
$STELLAR contract invoke --id "$PAYMENT" $SRC $NET -- init --authority "$IDENT" --tee_worker "$IDENT" --token "$NATIVE_XLM_SAC"
# USDC SAC for sentinel-sac (demo asset issued by $IDENT)
USDC=$($STELLAR contract asset deploy --asset "USDC:$IDENT" $SRC $NET | tail -1)
$STELLAR contract invoke --id "$SAC" $SRC $NET -- init --authority "$IDENT" --tee_worker "$IDENT" --asset "$USDC"

echo "DONE"
echo "VAULT=$VAULT"
echo "ORACLE=$ORACLE"
echo "PAYMENT=$PAYMENT"
echo "SAC=$SAC"
echo "USDC=$USDC"
