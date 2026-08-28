#!/usr/bin/env bash
# =============================================================================
# IGORFAKE (TC) — removes the OBSOLETE route of domain 1399811151 (Solana devnet)
#
# Requested by the hyperlane-registry PR #1559 reviewer (paulbalaji): the TC
# router has 4 enrolled routes (1, 56, 1399811149 and 1399811151-devnet), but the
# PR declares only the 3 production ones. This script unenrolls the devnet (route: null).
#
#   usage: bash deploy/tc-unenroll-devnet-route.sh
#
# ⚠️ After running, RESTART THE RELAYER on the VPS (same key = sequence):
#     ssh root@31.97.91.4 'systemctl restart hyperlane-relayer'
# =============================================================================
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
NODE=https://rpc.terra-classic.hexxagon.io:443
WARP=terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)

read -rs -p "Keyring password (key $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$OPERADOR" ] || { echo "❌ key $KEY = $ADDR, expected $OPERADOR"; exit 1; }

echo "== routes BEFORE =="
terrad q wasm contract-state smart "$WARP" '{"router":{"list_routes":{}}}' --node "$NODE" --output json | python3 -m json.tool | grep -E "domain|route"

echo "== removing route of domain 1399811151 (devnet) =="
H=$(sign tx wasm execute "$WARP" '{"router":{"set_route":{"set":{"domain":1399811151,"route":null}}}}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
echo "tx: $H — waiting…"
for i in $(seq 1 20); do R=$(terrad q tx "$H" --node "$NODE" --output json 2>/dev/null) && break; sleep 3; done
echo "$R" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0, r.get("raw_log"); print("✓ confirmed, height", r["height"])'

echo "== routes AFTER (should have only 1, 56 and 1399811149) =="
terrad q wasm contract-state smart "$WARP" '{"router":{"list_routes":{}}}' --node "$NODE" --output json | python3 -m json.tool | grep -E "domain|route"
echo
echo "⚠️ now restart the relayer: ssh root@31.97.91.4 'systemctl restart hyperlane-relayer'"
