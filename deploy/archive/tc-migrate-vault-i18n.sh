#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault i18n migration on TC (EN string translation only)
#
# Only the relayer-reward-vault changed bytecode (2 production error strings were
# translated PT->EN). The oracle-governor, EVM and Solana are byte-identical -> no
# migration. This does: store the new wasm -> migrate AT THE SAME address with an
# EMPTY MigrateMsg (pool/beneficiary/state fully preserved). Reversible: migrate
# back to the previous code_id (11596).
#
#   usage:  KEY=<key_name_in_terrad> bash deploy/tc-migrate-vault-i18n.sh
#
# Signs with the LOCAL terrad keyring key of the admin (terra1run9wz...). Do NOT run
# on the VPS (project rule). Build the wasm FIRST (reproducible, optimizer 0.17.0):
#   docker run --rm -v "$(pwd)":/code -v cwopt_cache:/target \
#     -v cwopt_registry:/usr/local/cargo/registry cosmwasm/optimizer:0.17.0
# =============================================================================
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"          # terrad keyring key = terra1run9wz... (vault admin)
KEYRING="file"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
ADMIN=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
WASM="$ROOT/artifacts/relayer_reward_vault.wasm"
EXPECT=339b82571a9679830f1b7469a2ae42a96929286d77954f53014416af9bcc33fa   # new data_hash (0.17.0)
PREV_CODE=11596                                                            # rollback target
TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)

[ -f "$WASM" ] || { echo "❌ $WASM not found — build it first (see header)"; exit 1; }
GOT=$(sha256sum "$WASM" | cut -d' ' -f1)
echo "wasm sha256: $GOT"
[ "$GOT" = "$EXPECT" ] || { echo "⚠️  sha differs from the reviewed $EXPECT — rebuild with optimizer 0.17.0 and review before proceeding"; exit 1; }

read -rs -p "Keyring password (key $KEY): " PASS; echo
sign() { printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$ADMIN" ] || { echo "❌ key $KEY = $ADDR, expected admin $ADMIN"; exit 1; }
echo "✓ key matches admin: $ADDR"
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }

echo "== 1/3 store the new wasm =="
H=$(sign tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
R=$(wait_tx "$H")
CODE=$(echo "$R" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print([{a["key"]:a["value"] for a in e["attributes"]}["code_id"] for e in r["events"] if e["type"]=="store_code"][0])')
echo "✓ new code_id: $CODE · tx $H"

echo "== 2/3 migrate the vault (empty MigrateMsg — state preserved) =="
H=$(sign tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
R=$(wait_tx "$H")
echo "$R" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print("✓ migrated · tx",r["txhash"])'

echo "== 3/3 verify =="
terrad q wasm contract "$VAULT" --node "$NODE" --output json | python3 -c 'import json,sys;d=json.load(sys.stdin)["contract_info"];print("code_id now:",d["code_id"])'
terrad q wasm contract-state smart "$VAULT" '{"solvency":{}}' --node "$NODE" --output json 2>/dev/null | head -c 200; echo
echo "✅ done. Rollback if needed:  terrad tx wasm migrate $VAULT $PREV_CODE '{}' ${TX[*]}"
