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
# Keypair: owner atual do IGP (BirXd4…Ef1j). Custo estimado do deploy: ~2 SOL
# de rent dos programas (recuperável via close se abortar).
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
  RRV=$(get_state RRV_ID); GOV=$(get_state GOV_ID)
  [ -n "$RRV" ] && [ -n "$GOV" ] || { echo "❌ rode o deploy antes"; exit 1; }
  say "FINALIZE: transfer IGP + beneficiary + seed (você TESTOU em devnet? ctrl-c se não)"
  sleep 5
  SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$RRV" "$GOV" --transfer-igp --set-beneficiary --seed
  echo; echo "⚠️ ÚLTIMO PASSO DE SEGURANÇA (manual, quando o multisig existir):"
  echo "   solana program set-upgrade-authority $RRV --new-upgrade-authority <MULTISIG> -k $KEYPAIR -u $RPC"
  echo "   solana program set-upgrade-authority $GOV --new-upgrade-authority <MULTISIG> -k $KEYPAIR -u $RPC"
  exit 0
fi

say "1/3 build-sbf (usa os .so já buildados se presentes)"
[ -f "$ROOT/svm/target/deploy/rrv.so" ] || (cd "$ROOT/svm" && cargo build-sbf)
ls -la "$ROOT"/svm/target/deploy/*.so | grep -v mock

if ! done_step RRV_ID; then
  say "2/3 deploy rrv.so"
  out=$(solana program deploy "$ROOT/svm/target/deploy/rrv.so" -k "$KEYPAIR" -u "$RPC" --output json)
  mark RRV_ID "$(echo "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["programId"])')"
fi
echo "✓ rrv program: $(get_state RRV_ID)"

if ! done_step GOV_ID; then
  say "3/3 deploy igp_oracle_governor.so"
  out=$(solana program deploy "$ROOT/svm/target/deploy/igp_oracle_governor.so" -k "$KEYPAIR" -u "$RPC" --output json)
  mark GOV_ID "$(echo "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["programId"])')"
fi
echo "✓ governor program: $(get_state GOV_ID)"

say "init (rrv + governor + domínio 132556 + top-up da config PDA)"
SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$(get_state RRV_ID)" "$(get_state GOV_ID)"

say "PRÓXIMOS PASSOS"
echo "1. TESTE EM DEVNET a devolução de posse (spec §08 — obrigatório):"
echo "   deploy dos mesmos programas em devnet + TransferIgpOwnership ida e volta"
echo "2. bash deploy/solana-deploy.sh finalize   # transfer + beneficiary + seed"
echo "3. upgrade authority → multisig (comando impresso no finalize)"
