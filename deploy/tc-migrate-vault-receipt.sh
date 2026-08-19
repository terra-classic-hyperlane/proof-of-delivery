#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault RECIBO TRUSTLESS (TC) — migrate + config LOCAL
#
#   uso:  BSC_VAULT=0x<addr> bash deploy/tc-migrate-vault-receipt.sh
#
# Migra o vault do TC (MESMO endereço, pool preservado) p/ o wasm com
# send_receipt/handle e configura o lado TC do corredor TC↔BSC:
#   - operator_address[0][132556] = terra1run9wz… (payout + reverse-lookup)
#   - operator_address[0][56]      = 0x8f08… (registro)
#   - remote_router[56]            = vault da BSC (hex32 left-pad) — confia/despacha
#   - remote_reward[56]            = taxa real de origem TC→BSC (LUNC)
# Assina com a key hyperlane-deploy (keyring file). NADA disso na VPS.
# =============================================================================
set -euo pipefail
: "${BSC_VAULT:?exporte BSC_VAULT=0x<endereco_do_vault_recibo_da_bsc>}"
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
OPERADOR_BSC="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
WASM="$ROOT/artifacts/relayer_reward_vault.wasm"
# recompensa TC→BSC em uluna (taxa real de origem; ajuste com REWARD_TC=)
REWARD_TC="${REWARD_TC:-33000000}"
# vault da BSC como router hex32 (EVM 20B → left-pad 32B), minúsculo
BSC_HEX40=$(echo "$BSC_VAULT" | sed 's/^0x//' | tr 'A-Z' 'a-z')
BSC_ROUTER="0x$(printf '%024d' 0)$BSC_HEX40"

TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)
STATE="$ROOT/deploy/tc-receipt.state"; touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }; done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
read -rs -p "Senha do keyring (chave $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$OPERADOR" ] || { echo "❌ chave $KEY = $ADDR, esperado $OPERADOR"; exit 1; }
echo "✓ chave confere: $ADDR"
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }

echo "wasm: $(sha256sum "$WASM" | cut -d' ' -f1)"
echo "BSC router (hex32): $BSC_ROUTER"

if ! done_step CODE_ID; then
  say "1/6 store do wasm (recibo)"
  H=$(sign tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  CODE=$(wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print([{a["key"]:a["value"] for a in e["attributes"]}["code_id"] for e in r["events"] if e["type"]=="store_code"][0])')
  mark CODE_ID "$CODE"; echo "✓ code_id $CODE"
fi
CODE=$(get_state CODE_ID)

if ! done_step MIGRATED; then
  say "2/6 migrate (mesmo endereço, pool preservado)"
  H=$(sign tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  wait_tx "$H" | python3 -c 'import json,sys;assert json.load(sys.stdin).get("code")==0'
  mark MIGRATED "$H"; echo "✓ migrate: $H"
fi

exec_msg(){ done_step "$1" && return; H=$(sign tx wasm execute "$VAULT" "$2" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])'); wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log")'; mark "$1" "$H"; echo "✓ $1: $H"; }

say "3/6 registro de/para + router + recompensa (lado TC)"
exec_msg OP_TC    "{\"set_operator_address\":{\"index\":0,\"domain\":132556,\"address\":\"$OPERADOR\"}}"
exec_msg OP_BSC   "{\"set_operator_address\":{\"index\":0,\"domain\":56,\"address\":\"$OPERADOR_BSC\"}}"
exec_msg ROUTER   "{\"set_remote_router\":{\"domain\":56,\"address\":\"$BSC_ROUTER\"}}"
exec_msg REWARD   "{\"set_remote_reward\":{\"domain\":56,\"reward\":\"$REWARD_TC\"}}"

say "VERIFICAÇÃO"
Q(){ terrad q wasm contract-state smart "$VAULT" "$1" --node "$NODE" --output json | python3 -c 'import json,sys;print(json.dumps(json.load(sys.stdin)["data"]))'; }
echo "router[56]:      $(Q '{"remote_router":{"domain":56}}')"
echo "reward[56]:      $(Q '{"remote_reward":{"domain":56}}')"
echo "op0 no TC:       $(Q '{"operator_address":{"index":0,"domain":132556}}')"
echo "reverse-lookup:  $(Q "{\"operator_of_local\":{\"address\":\"$OPERADOR\"}}")"
say "LADO TC CONFIGURADO 🎉  corredor TC↔BSC pronto p/ o teste"
echo "PENDÊNCIAS: (1) rota Hyperlane (vault como recipient/ISM já cobre TC↔BSC);"
echo "            (2) semear os pools (TC já tem; BSC: cast send --legacy $BSC_VAULT --value <wei> …)"
