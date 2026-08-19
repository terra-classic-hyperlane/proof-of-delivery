#!/usr/bin/env bash
# Re-migra o vault do TC para o wasm ATUAL (correção da query ISM), preservando
# TODO o estado (pool, registro de/para, routers, rewards). Só store + migrate.
#   uso:  bash deploy/tc-remigrate.sh
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
WASM="$ROOT/artifacts/relayer_reward_vault.wasm"
TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
read -rs -p "Senha do keyring (chave $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
[ "$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")" = "$OPERADOR" ] || { echo "❌ chave errada"; exit 1; }
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }

echo "wasm: $(sha256sum "$WASM" | cut -d' ' -f1)"
say "store"
H=$(sign tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
CODE=$(wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print([{a["key"]:a["value"] for a in e["attributes"]}["code_id"] for e in r["events"] if e["type"]=="store_code"][0])')
echo "✓ code_id $CODE"
say "migrate (estado preservado)"
H=$(sign tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log")'
echo "✓ migrate: $H"
say "VERIFICAÇÃO (a query de ISM agora responde)"
terrad q wasm contract-state smart "$VAULT" '{"ism_specifier":{"interchain_security_module":[]}}' --node "$NODE" --output json | python3 -c 'import json,sys;print("ism_specifier →",json.load(sys.stdin)["data"])'
echo "✓ recibo em voo será entregue na próxima tentativa do relayer"
