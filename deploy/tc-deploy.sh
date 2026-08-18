#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Deploy Fases 1–2 no Terra Classic (columbus-5)
#
#   FASE 1: oracle-governor + posse do StorageGasOracle + faixas por domínio
#   FASE 2: relayer-reward-vault + IGP.beneficiary = vault + semente do pool
#
# Assina com a chave "hyperlane-deploy" (keyring file — a senha é pedida UMA
# vez e reutilizada via pipe; nunca é gravada em lugar nenhum).
# Parâmetros: docs/PARAMETROS_PROPOSTA.md. Owner inicial = deployer (handoff
# p/ governança depois — seção 8 do mesmo doc).
# =============================================================================
set -euo pipefail

NODE="https://terra-classic-rpc.publicnode.com:443"
CHAIN="columbus-5"
KEY="hyperlane-deploy"
KEYRING="file"
DEPLOYER="terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp"

MAILBOX="terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9"
IGP="terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz"
IGP_ORACLE="terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM_GOV="$ROOT/artifacts/oracle_governor.wasm"
WASM_VAULT="$ROOT/artifacts/relayer_reward_vault.wasm"
STATE="$ROOT/deploy/tc-deploy.state"   # progresso p/ retomar se algo falhar

TXFLAGS=(--node "$NODE" --chain-id "$CHAIN" --from "$KEY" --keyring-backend "$KEYRING"
         --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna
         --broadcast-mode sync --output json -y)

# ---------- helpers ----------
say() { printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
jget() { python3 -c "import sys,json;d=json.load(sys.stdin);print(eval(sys.argv[1]))" "$1"; }

read -rs -p "Senha do keyring (chave $KEY): " PASS; echo
sign() { printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }

ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$DEPLOYER" ] || { echo "❌ chave $KEY = $ADDR, esperado $DEPLOYER"; exit 1; }
echo "✓ chave confere: $ADDR"

wait_tx() { # hash → json da tx incluída (falha se code!=0)
  local hash="$1" out code
  for _ in $(seq 1 40); do
    if out=$(terrad q tx "$hash" --node "$NODE" --output json 2>/dev/null); then
      code=$(echo "$out" | jget "d['code']")
      [ "$code" = "0" ] || { echo "❌ tx $hash falhou (code $code):" >&2; echo "$out" | jget "d['raw_log'][:400]" >&2; exit 1; }
      echo "$out"; return 0
    fi; sleep 3
  done
  echo "❌ timeout esperando tx $hash" >&2; exit 1
}

tx() { # executa terrad tx ..., espera inclusão, ecoa o json final
  local res hash
  res=$(sign tx "$@" "${TXFLAGS[@]}")
  hash=$(echo "$res" | jget "d['txhash']")
  echo "  tx: $hash" >&2
  wait_tx "$hash"
}

event_attr() { # json, tipo, chave → valor (primeira ocorrência)
  python3 -c "
import sys,json
d=json.load(sys.stdin)
for ev in d.get('events',[]):
    if ev['type']==sys.argv[1]:
        for a in ev['attributes']:
            if a['key']==sys.argv[2]: print(a['value']); sys.exit(0)
for log in d.get('logs',[]):
    for ev in log.get('events',[]):
        if ev['type']==sys.argv[1]:
            for a in ev['attributes']:
                if a['key']==sys.argv[2]: print(a['value']); sys.exit(0)
sys.exit(1)" "$1" "$2"
}

mark() { echo "$1=$2" >> "$STATE"; }
done_step() { grep -q "^$1=" "$STATE" 2>/dev/null; }
get_state() { grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
touch "$STATE"

# ---------- FASE 1 ----------
if ! done_step CODE_GOV; then
  say "1/9 store oracle_governor.wasm"
  out=$(tx wasm store "$WASM_GOV")
  code_id=$(echo "$out" | event_attr store_code code_id)
  mark CODE_GOV "$code_id"
fi
CODE_GOV=$(get_state CODE_GOV); echo "✓ oracle-governor code_id: $CODE_GOV"

if ! done_step CODE_VAULT; then
  say "2/9 store relayer_reward_vault.wasm"
  out=$(tx wasm store "$WASM_VAULT")
  code_id=$(echo "$out" | event_attr store_code code_id)
  mark CODE_VAULT "$code_id"
fi
CODE_VAULT=$(get_state CODE_VAULT); echo "✓ vault code_id: $CODE_VAULT"

say "verificando data_hash on-chain vs checksums.txt"
for pair in "$CODE_GOV:oracle_governor.wasm" "$CODE_VAULT:relayer_reward_vault.wasm"; do
  cid="${pair%%:*}"; f="${pair##*:}"
  onchain=$(terrad q wasm code-info "$cid" --node "$NODE" --output json | jget "d['data_hash']" | tr a-z A-Z)
  local_hash=$(grep "$f" "$ROOT/artifacts/checksums.txt" | cut -d' ' -f1 | tr a-z A-Z)
  [ "$onchain" = "$local_hash" ] || { echo "❌ data_hash divergente p/ $f! on-chain=$onchain local=$local_hash"; exit 1; }
  echo "✓ $f data_hash confere ($onchain)"
done

if ! done_step GOV_ADDR; then
  say "3/9 instantiate oracle-governor"
  # operadores: deployer + (opcional) OPERATOR2 via env; quórum acompanha (docs/OPERADORES.md)
  OPS="\"$DEPLOYER\""; Q=1
  if [ -n "${OPERATOR2:-}" ]; then OPS="$OPS,\"$OPERATOR2\""; Q=${QUORUM:-2}; fi
  init_gov=$(cat <<JSON
{"owner":"$DEPLOYER","oracle":"$IGP_ORACLE","operators":[$OPS],"quorum":$Q,"epoch_duration_secs":21600,"max_delta_bps":2000}
JSON
)
  out=$(tx wasm instantiate "$CODE_GOV" "$init_gov" --label "hpl-oracle-governor" --admin "$DEPLOYER")
  addr=$(echo "$out" | event_attr instantiate _contract_address)
  mark GOV_ADDR "$addr"
fi
GOV_ADDR=$(get_state GOV_ADDR); echo "✓ oracle-governor: $GOV_ADDR"

if ! done_step ORACLE_TRANSFER; then
  say "4/9 posse do StorageGasOracle → governor (passo 1: init transfer)"
  tx wasm execute "$IGP_ORACLE" "{\"ownership\":{\"init_ownership_transfer\":{\"next_owner\":\"$GOV_ADDR\"}}}" >/dev/null
  mark ORACLE_TRANSFER ok
fi
if ! done_step ORACLE_CLAIM; then
  say "5/9 posse do StorageGasOracle → governor (passo 2: claim)"
  tx wasm execute "$GOV_ADDR" '{"claim_oracle_ownership":{}}' >/dev/null
  mark ORACLE_CLAIM ok
fi
echo "✓ governor é owner do oracle"

if ! done_step BOUNDS; then
  say "6/9 faixas por domínio — DERIVADAS DO ORACLE EM PRODUÇÃO neste momento (vigente ÷3 · ×3)"
  # Nada de valor fixo: a doc envelhece; a fonte é o que está NO ORACLE agora.
  set_bounds() { tx wasm execute "$GOV_ADDR" "{\"set_bounds\":{\"domain\":$1,\"bounds\":{\"min_exchange_rate\":\"$2\",\"max_exchange_rate\":\"$3\",\"min_gas_price\":\"$4\",\"max_gas_price\":\"$5\"}}}" >/dev/null; }
  for dom in 1 56 1399811149; do
    vals=$(terrad q wasm contract-state smart "$IGP_ORACLE" \
      "{\"oracle\":{\"get_exchange_rate_and_gas_price\":{\"dest_domain\":$dom}}}" \
      --node "$NODE" --output json | python3 -c '
import sys, json
d = json.load(sys.stdin)["data"]
rate, gas = int(d["exchange_rate"]), int(d["gas_price"])
assert rate > 0 and gas > 0, "oracle sem valor para o domínio — configure-o antes"
print(max(1, rate // 3), rate * 3, max(1, gas // 3), gas * 3)')
    read -r MIN_R MAX_R MIN_G MAX_G <<< "$vals"
    echo "  dom $dom: vigente lido do oracle → faixa rate [$MIN_R · $MAX_R] · gas [$MIN_G · $MAX_G]"
    set_bounds "$dom" "$MIN_R" "$MAX_R" "$MIN_G" "$MAX_G"
  done
  mark BOUNDS ok
fi
echo "✓ faixas definidas (dom 1, 56, 1399811149)"

# ---------- FASE 2 ----------
if ! done_step VAULT_ADDR; then
  say "7/9 instantiate relayer-reward-vault (50 LUNC/entrega · janela 200k blocos)"
  init_vault=$(cat <<JSON
{"owner":"$DEPLOYER","mailbox":"$MAILBOX","igp":"$IGP","denom":"uluna","reward_per_delivery":"50000000","claim_window_blocks":200000}
JSON
)
  out=$(tx wasm instantiate "$CODE_VAULT" "$init_vault" --label "hpl-relayer-reward-vault" --admin "$DEPLOYER")
  addr=$(echo "$out" | event_attr instantiate _contract_address)
  mark VAULT_ADDR "$addr"
fi
VAULT_ADDR=$(get_state VAULT_ADDR); echo "✓ vault: $VAULT_ADDR"

if ! done_step BENEFICIARY; then
  say "8/9 IGP.beneficiary = vault"
  tx wasm execute "$IGP" "{\"set_beneficiary\":{\"beneficiary\":\"$VAULT_ADDR\"}}" >/dev/null
  mark BENEFICIARY ok
fi
echo "✓ beneficiary apontado"

if ! done_step SEED; then
  say "9/9 semente do pool: 5.000 LUNC"
  tx bank send "$KEY" "$VAULT_ADDR" 5000000000uluna >/dev/null
  mark SEED ok
fi
echo "✓ pool semeado"

# ---------- verificação final ----------
say "VERIFICAÇÃO"
q() { terrad q wasm contract-state smart "$1" "$2" --node "$NODE" --output json; }
echo "-- governor config:"; q "$GOV_ADDR" '{"config":{}}' | jget "json.dumps(d['data'])"
echo "-- oracle owner:";   q "$IGP_ORACLE" '{"ownable":{"get_owner":{}}}' | jget "d['data']"
echo "-- vault config:";   q "$VAULT_ADDR" '{"config":{}}' | jget "json.dumps(d['data'])"
echo "-- vault solvency:"; q "$VAULT_ADDR" '{"solvency":{}}' | jget "json.dumps(d['data'])"
echo "-- layout_check (mensagem real d039daa1…):"
q "$VAULT_ADDR" '{"layout_check":{"message_id":"d039daa1c75d5b558906fef6d790b13dc94a8b39e58e1e7f219b3967a28c4f04"}}' | jget "json.dumps(d['data'])"
echo "-- igp beneficiary:"; q "$IGP" '{"beneficiary":{}}' | jget "d['data']" || true

say "DEPLOY CONCLUÍDO 🎉  (endereços salvos em $STATE)"
