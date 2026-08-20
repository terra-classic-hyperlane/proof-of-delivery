#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault TC — migrate: recibo paga GÁS REAL, não a tarifa
#
#   uso:  bash deploy/tc-migrate-vault-gas-recibo.sh
#         SKIP_BUILD=1 …   (reusa artifacts/relayer_reward_vault.wasm já buildado)
#         SKIP_VPS=1 …     (não atualiza o claim-agent na VPS)
#
# Contexto: a tarifa de usuário virou ~$0,08 (gas_for_domain no IGP), mas o IGP
# cobrava isso de TODO dispatch do TC — inclusive dos RECIBOS, devorando a
# comissão do operador (BSC→TC e SOL→TC ficavam negativos). Este migrate adiciona
# `gas_limit` ao SendReceipt: o vault passa metadata ao IGP (32B BE) e o recibo
# paga só o gás real de entrega (~$0,001-0,005). Warp continua pagando $0,08
# (o hpl_warp não expõe metadata — usuário não burla).
#
# MESMO endereço, pool preservado. Assina com a key hyperlane-deploy (keyring
# file). Depois: atualiza o claim-agent na VPS (cota o IGP dinamicamente).
# Monitorar LayoutCheck após o migrate (spec §06) — o script já verifica.
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
  say "0/4 build reproduzível (cosmwasm/optimizer:0.17.0 — mesmo dos wasms em produção)"
  ( cd "$ROOT" && docker run --rm -v "$(pwd)":/code \
      --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
      --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
      cosmwasm/optimizer:0.17.0 )
fi
[ -f "$WASM" ] || { echo "❌ $WASM não existe — rode sem SKIP_BUILD"; exit 1; }
echo "wasm: $(sha256sum "$WASM" | cut -d' ' -f1)"

read -rs -p "Senha do keyring (chave $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$OPERADOR" ] || { echo "❌ chave $KEY = $ADDR, esperado $OPERADOR"; exit 1; }
echo "✓ chave confere: $ADDR"
wait_tx(){ local h=$1; for i in $(seq 1 20); do R=$(terrad q tx "$h" --node "$NODE" --output json 2>/dev/null) && { echo "$R"; return; }; sleep 3; done; echo '{"code":-1}'; }

if ! done_step CODE_ID; then
  say "1/4 store do wasm"
  H=$(sign tx wasm store "$WASM" "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  CODE=$(wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log");print([{a["key"]:a["value"] for a in e["attributes"]}["code_id"] for e in r["events"] if e["type"]=="store_code"][0])')
  mark CODE_ID "$CODE"; echo "✓ code_id $CODE"
fi
CODE=$(get_state CODE_ID)

if ! done_step MIGRATED; then
  say "2/4 migrate (mesmo endereço, pool preservado)"
  H=$(sign tx wasm migrate "$VAULT" "$CODE" '{}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
  wait_tx "$H" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0,r.get("raw_log")'
  mark MIGRATED "$H"; echo "✓ migrate: $H"
fi

say "3/4 verificação pós-migrate"
Q(){ terrad q wasm contract-state smart "$VAULT" "$1" --node "$NODE" --output json | python3 -c 'import json,sys;print(json.dumps(json.load(sys.stdin)["data"]))'; }
echo "config:   $(Q '{"config":{}}')"
echo "solvency: $(Q '{"solvency":{}}')"
# LayoutCheck contra uma entrega SABIDAMENTE processada (spec §06) — pega o id
# mais recente do próprio mailbox via tx_search
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
  Q "{\"layout_check\":{\"message_id\":\"$KNOWN_ID\"}}" || echo "⚠ layout_check falhou — INVESTIGAR antes de qualquer pagamento"
else
  echo "⚠ sem KNOWN_ID — rode: KNOWN_ID=<message_id_hex> e confira o layout_check"
fi

if [ -z "${SKIP_VPS:-}" ]; then
  say "4/4 claim-agent novo na VPS (cota o IGP dinamicamente + gas_limit)"
  scp "$ROOT/deploy/claim-agent-receipt.mjs" "$VPS:/root/claim-agent/claim-agent-receipt.mjs"
  ssh "$VPS" 'systemctl restart claim-agent && sleep 3 && systemctl is-active claim-agent && tail -n 3 /root/claim-agent/logs/agent.log'
fi

say "PRONTO 🎉  recibos agora pagam gás real (~100 LUNC p/ BSC, ~20 p/ SOL)"
echo "Acompanhe 1 rodada: ssh $VPS 'tail -f /root/claim-agent/logs/agent.log'"
echo "Os 2 recibos pendentes (origem 56 e 1399811149) devem sair na próxima rodada."
