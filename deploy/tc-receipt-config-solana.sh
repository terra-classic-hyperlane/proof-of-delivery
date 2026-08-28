#!/usr/bin/env bash
# Config of the RECEIPT MODEL on the TC vault — Solana->TC corridor.
# Registers (in YOUR contract, without touching anything native):
#   1. SetRemoteRouter(1399811149, <pod 32B>) → WHERE send_receipt dispatches the
#      receipt (the `pod` on Solana), when the origin of the delivered msg is Solana.
#   2. SetOperatorAddress(index=0, domain=132556, terra1run…) → to/from + reverse
#      lookup (OPERATOR_OF_LOCAL) that send_receipt uses to find the local
#      executor index. Idempotent (may already be set in production).
#
#   usage:  bash deploy/tc-receipt-config-solana.sh
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
DOM_SOL=1399811149
DOM_TC=132556
# pod (Solana) as 32 bytes (base58 pubkey → hex) — receipt recipient
POD_32=0x1a3be2685e7a787a1bedadcc90889b367f8fe72240de5aa43e4c2b88d07776a2
OP_INDEX=0

TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
read -rs -p "Keyring password (key $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
[ "$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")" = "$OPERADOR" ] || { echo "❌ wrong key"; exit 1; }
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }
exec_msg(){ # $1 = json
  H=$(sign tx wasm execute "$VAULT" "$1" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log")'
  echo "  ✓ $H"
}

say "1) router to Solana (receipt destination) = pod"
exec_msg "{\"set_remote_router\":{\"domain\":$DOM_SOL,\"address\":\"$POD_32\"}}"

say "2) to/from of the local operator (index $OP_INDEX → $OPERADOR)"
exec_msg "{\"set_operator_address\":{\"index\":$OP_INDEX,\"domain\":$DOM_TC,\"address\":\"$OPERADOR\"}}"

say "VERIFICATION"
terrad q wasm contract-state smart "$VAULT" "{\"remote_router\":{\"domain\":$DOM_SOL}}" --node "$NODE" --output json | python3 -c 'import json,sys;print("router[solana] →",json.load(sys.stdin)["data"])'
echo "✓ TC side ready. Now the operator delivers Solana->TC (native relayer) and calls send_receipt."
