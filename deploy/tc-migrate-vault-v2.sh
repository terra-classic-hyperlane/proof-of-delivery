#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault v2 (ClaimRemote) — migração LOCAL no TC
#
#   uso:  KEY=<nome_da_key_no_terrad> bash deploy/tc-migrate-vault-v2.sh
#
# Faz: store do wasm v2 → migrate NO MESMO endereço (pool/beneficiary intactos)
# → configura atestador + vínculos das 3 chains + recompensas (33 LUNC/domínio)
# → atesta as 3 entregas de 19/08 (SOL/BSC/ETH) = 99 LUNC ao operador.
# Idempotente via deploy/tc-v2.state. Assina com a key do KEYRING LOCAL do
# terrad (o owner/admin terra1run9wz…). NADA disso roda na VPS (regra do projeto).
# =============================================================================
set -euo pipefail
: "${KEY:?uso: KEY=<key_no_terrad> bash deploy/tc-migrate-vault-v2.sh}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE=https://rpc.terra-classic.hexxagon.io
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
WASM="$ROOT/artifacts/relayer_reward_vault.wasm"
TX=(--from "$KEY" --gas auto --gas-adjustment 1.4 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)
STATE="$ROOT/deploy/tc-v2.state"
touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }
done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
wait_tx(){ local h=$1; for i in $(seq 1 20); do
  R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done
  echo '{"code":-1}'; }

echo "wasm: $(sha256sum "$WASM" | cut -d' ' -f1) (esperado e24a5e66ab4a503c6acf369710b717310362d2ae5fa7b9800542c8272b2fc801)"

if ! done_step CODE_ID; then
  say "1/4 store do wasm v2"
  H=$(terrad tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  R=$(wait_tx "$H")
  CODE=$(echo "$R" | python3 -c '
import json,sys
r=json.load(sys.stdin)
assert r.get("code")==0, r.get("raw_log","tx falhou")
for e in r["events"]:
    if e["type"]=="store_code":
        print({a["key"]:a["value"] for a in e["attributes"]}["code_id"]); break')
  mark CODE_ID "$CODE"; echo "✓ code_id $CODE · tx $H"
fi
CODE=$(get_state CODE_ID); echo "code_id: $CODE"

if ! done_step MIGRATED; then
  say "2/4 migrate (mesmo endereço — pool e beneficiary preservados)"
  H=$(terrad tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  R=$(wait_tx "$H"); echo "$R" | python3 -c 'import json,sys; r=json.load(sys.stdin); assert r.get("code")==0, r.get("raw_log")'
  mark MIGRATED "$H"; echo "✓ migrate: $H"
fi

exec_msg(){ # $1=step $2=json
  done_step "$1" && return
  H=$(terrad tx wasm execute "$VAULT" "$2" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  R=$(wait_tx "$H"); echo "$R" | python3 -c 'import json,sys; r=json.load(sys.stdin); assert r.get("code")==0, r.get("raw_log")'
  mark "$1" "$H"; echo "✓ $1: $H"
}

say "3/4 configuração remota (atestador · vínculos TC↔BSC/ETH/SOL · recompensas)"
exec_msg OPS      "{\"set_remote_operators\":{\"attestors\":[\"$OPERADOR\"],\"quorum\":1}}"
exec_msg BIND_SOL "{\"set_remote_binding\":{\"operator\":\"$OPERADOR\",\"domain\":1399811149,\"remote_address\":\"PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS\"}}"
exec_msg BIND_BSC "{\"set_remote_binding\":{\"operator\":\"$OPERADOR\",\"domain\":56,\"remote_address\":\"0x8f085bad1a15ee9ceee58c83efffa72518975291\"}}"
exec_msg BIND_ETH "{\"set_remote_binding\":{\"operator\":\"$OPERADOR\",\"domain\":1,\"remote_address\":\"0xef8181201ce6c83120035ffbcc11945e67ba00ae\"}}"
exec_msg RW_SOL   '{"set_remote_reward":{"domain":1399811149,"reward":"33000000"}}'
exec_msg RW_BSC   '{"set_remote_reward":{"domain":56,"reward":"33000000"}}'
exec_msg RW_ETH   '{"set_remote_reward":{"domain":1,"reward":"33000000"}}'

say "4/4 atesta as 3 entregas de 19/08 (SOL · BSC · ETH) — 33 LUNC cada"
exec_msg ATT_SOL '{"attest_remote_delivery":{"domain":1399811149,"message_ids":["1e070a74d52a27d901d62aa1b4f71d3bff48e3e3f6d2d6b27e6f0ea5dbe2a01d"]}}'
exec_msg ATT_BSC '{"attest_remote_delivery":{"domain":56,"message_ids":["72f1099dfe8a91cd6a5e1a3ebcacd276e85060af9dc9e9b689ead18d24441a58"]}}'
exec_msg ATT_ETH '{"attest_remote_delivery":{"domain":1,"message_ids":["6c6518b02df5c9928150448b01a70e5f4a1c292bdcf81c7230c5cc20359bce9f"]}}'

say "VERIFICAÇÃO"
terrad q wasm contract-state smart "$VAULT" '{"remote_config":{}}' --node "$NODE"
say "V2 ATIVA — 99 LUNC pagos pelas 3 entregas remotas de hoje 🎉"
