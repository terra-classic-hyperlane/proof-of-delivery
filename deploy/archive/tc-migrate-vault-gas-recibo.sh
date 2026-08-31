#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault TC — migrate: receipt pays REAL GAS, not the fee
#
#   usage:  bash deploy/tc-migrate-vault-gas-recibo.sh
#           SKIP_BUILD=1 …   (reuses the already-built artifacts/relayer_reward_vault.wasm)
#           SKIP_VPS=1 …     (does not update the claim-agent on the VPS)
#
# Context: the user fee became ~$0.08 (gas_for_domain in the IGP), but the IGP
# charged that on EVERY TC dispatch — including the RECEIPTS, eating up the
# operator's commission (BSC→TC and SOL→TC went negative). This migrate adds
# `gas_limit` to SendReceipt: the vault passes metadata to the IGP (32B BE) and
# the receipt pays only the real delivery gas (~$0.001-0.005). Warp keeps paying $0.08
# (hpl_warp does not expose metadata — the user cannot cheat).
#
# SAME address, pool preserved. Signs with the hyperlane-deploy key (keyring
# file). Afterwards: updates the claim-agent on the VPS (quotes the IGP dynamically).
# Monitor LayoutCheck after the migrate (spec §06) — the script already checks it.
# =============================================================================
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
WASM="$ROOT/artifacts/relayer_reward_vault.wasm"
VPS="${VPS:-root@31.97.91.4}"

TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)
STATE="$ROOT/deploy/tc-gasrecibo.state"; touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }; done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

if [ -z "${SKIP_BUILD:-}" ]; then
  say "0/4 reproducible build (cosmwasm/optimizer:0.17.0 — same as the wasms in production)"
  ( cd "$ROOT" && docker run --rm -v "$(pwd)":/code \
      --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
      --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
      cosmwasm/optimizer:0.17.0 )
fi
[ -f "$WASM" ] || { echo "❌ $WASM does not exist — run without SKIP_BUILD"; exit 1; }
echo "wasm: $(sha256sum "$WASM" | cut -d' ' -f1)"

read -rs -p "Keyring password (key $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$OPERADOR" ] || { echo "❌ key $KEY = $ADDR, expected $OPERADOR"; exit 1; }
echo "✓ key matches: $ADDR"
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }

if ! done_step CODE_ID; then
  say "1/4 store of the wasm"
  H=$(sign tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  CODE=$(wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print([{a["key"]:a["value"] for a in e["attributes"]}["code_id"] for e in r["events"] if e["type"]=="store_code"][0])')
  mark CODE_ID "$CODE"; echo "✓ code_id $CODE"
fi
CODE=$(get_state CODE_ID)

if ! done_step MIGRATED; then
  say "2/4 migrate (same address, pool preserved)"
  H=$(sign tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log")'
  mark MIGRATED "$H"; echo "✓ migrate: $H"
fi

say "3/4 post-migrate verification"
Q(){ terrad q wasm contract-state smart "$VAULT" "$1" --node "$NODE" --output json | python3 -c 'import json,sys;print(json.dumps(json.load(sys.stdin)["data"]))'; }
echo "config:   $(Q '{"config":{}}')"
echo "solvency: $(Q '{"solvency":{}}')"
# LayoutCheck against a KNOWN-processed delivery (spec §06) — takes the most
# recent id from the mailbox itself via tx_search
KNOWN_ID="${KNOWN_ID:-$(curl -s "https://rpc.terra-classic.hexxagon.io/tx_search?query=%22wasm-mailbox_process._contract_address=%27terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9%27%22&per_page=1&order_by=%22desc%22" \
  | python3 -c 'import json,sys
r=json.load(sys.stdin)
for t in r["result"]["txs"]:
    for e in t["tx_result"]["events"]:
        if e["type"]=="wasm-mailbox_process_id":
            a={x["key"]:x["value"] for x in e["attributes"]}
            print(a.get("message_id","").removeprefix("0x")); raise SystemExit
' 2>/dev/null)}"
if [ -n "$KNOWN_ID" ]; then
  echo "layout_check($KNOWN_ID):"
  Q "{\"layout_check\":{\"message_id\":\"$KNOWN_ID\"}}" || echo "⚠ layout_check failed — INVESTIGATE before any payment"
else
  echo "⚠ no KNOWN_ID — run: KNOWN_ID=<message_id_hex> and check the layout_check"
fi

if [ -z "${SKIP_VPS:-}" ]; then
  say "4/4 new claim-agent on the VPS (quotes the IGP dynamically + gas_limit)"
  scp "$ROOT/deploy/claim-agent-receipt.mjs" "$VPS:/root/claim-agent/claim-agent-receipt.mjs"
  ssh "$VPS" 'systemctl restart claim-agent && sleep 3 && systemctl is-active claim-agent && tail -n 3 /root/claim-agent/logs/agent.log'
fi

say "DONE 🎉  receipts now pay real gas (~100 LUNC for BSC, ~20 for SOL)"
echo "Follow 1 round: ssh $VPS 'tail -f /root/claim-agent/logs/agent.log'"
echo "The 2 pending receipts (origin 56 and 1399811149) should go out in the next round."
