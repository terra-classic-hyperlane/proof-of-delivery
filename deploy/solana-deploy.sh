#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Deploy Fase 4 (Solana mainnet)
#
# ORDEM (spec §13): 1) programas + init (este script) → 2) TESTAR a devolução
# de posse EM DEVNET → 3) só então --transfer-igp / --set-beneficiary / --seed.
#
#   bash deploy/solana-deploy.sh            # deploy dos .so + init + domínio
#   bash deploy/solana-deploy.sh finalize   # transfer IGP + beneficiary + seed
#
# Keypair: owner atual do IGP (BirXd4…Ef1j).
# Custo: ~1,29 SOL de rent do pod.so (vault+governor FUNDIDOS num programa) +
# ~0,09 SOL de init/top-up. Rent recuperável via `solana program close`.
# =============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KEYPAIR="${SOLANA_KEYPAIR:-/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json}"
RPC="${SOLANA_RPC:-https://api.mainnet-beta.solana.com}"
STATE="$ROOT/deploy/solana.state"
touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }
done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

command -v solana >/dev/null || { echo "solana CLI ausente"; exit 1; }
say "signer: $(solana address -k "$KEYPAIR") · saldo: $(solana balance -k "$KEYPAIR" -u "$RPC")"

# symlink p/ o init usar os node_modules do oracle-agent
[ -e "$ROOT/deploy/node_modules" ] || ln -s ../oracle-agent/node_modules "$ROOT/deploy/node_modules"

if [ "${1:-}" = "finalize" ]; then
  POD=$(get_state POD_ID)
  [ -n "$POD" ] || { echo "❌ rode o deploy antes"; exit 1; }
  say "FINALIZE: transfer IGP + beneficiary + seed (você TESTOU em devnet? ctrl-c se não)"
  sleep 5
  SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$POD" --transfer-igp --set-beneficiary --seed
  echo; echo "⚠️ ÚLTIMO PASSO DE SEGURANÇA (manual, quando o multisig existir):"
  echo "   solana program set-upgrade-authority $POD --new-upgrade-authority <MULTISIG> -k $KEYPAIR -u $RPC"
  exit 0
fi

say "1/2 build-sbf (usa o pod.so já buildado se presente)"
[ -f "$ROOT/svm/target/deploy/pod.so" ] || (cd "$ROOT/svm" && cargo build-sbf)
ls -la "$ROOT"/svm/target/deploy/pod.so

# --max-len = tamanho EXATO do .so → rent pela METADE (sem os 2x de headroom de
# upgrade). Trade-off: upgrade só p/ binário <= tamanho atual; upgrade maior exige
# close+redeploy. Como a upgrade authority vai p/ multisig, é aceitável.
deploy_prog() {  # $1=arquivo.so $2=state_key $3=passo
  local so="$ROOT/svm/target/deploy/$1" key="$2"
  done_step "$key" && { echo "$(get_state "$key")"; return; }
  say "$3 deploy $1 (--max-len $(stat -c%s "$so"))"
  local out
  out=$(solana program deploy "$so" --max-len "$(stat -c%s "$so")" -k "$KEYPAIR" -u "$RPC" --output json)
  mark "$key" "$(echo "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["programId"])')"
}
# pod.so = vault + governor FUNDIDOS num programa só (a runtime solana+borsh,
# ~90% dos bytes, é paga UMA vez): rent 1,29 SOL vs 1,9 dos dois separados.
deploy_prog pod.so POD_ID "2/2" >/dev/null
echo "✓ pod program (vault+governor): $(get_state POD_ID)"

# VAULT_ONLY=1 → inicializa SÓ o módulo vault e aponta o beneficiary do IGP
# direto (sem governor). O preço segue com o owner do IGP até a Fase 4b.
if [ "${VAULT_ONLY:-0}" = "1" ]; then
  say "init (SÓ módulo vault — VAULT_ONLY)"
  SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$(get_state POD_ID)" --vault-only ${VAULT_ONLY_FLAGS:-}
  echo; echo "governor (já no binário) fica p/ a Fase 4b: rode sem VAULT_ONLY p/ inicializá-lo."
  exit 0
fi

say "init (vault + governor + domínio 132556 + top-up da config PDA)"
SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$(get_state POD_ID)"

say "PRÓXIMOS PASSOS"
echo "1. TESTE EM DEVNET a devolução de posse (spec §08 — obrigatório):"
echo "   deploy dos mesmos programas em devnet + TransferIgpOwnership ida e volta"
echo "2. bash deploy/solana-deploy.sh finalize   # transfer + beneficiary + seed"
echo "3. upgrade authority → multisig (comando impresso no finalize)"
