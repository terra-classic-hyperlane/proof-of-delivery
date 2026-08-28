#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Phase 4 Deploy (Solana mainnet)
#
# ORDER (spec §13): 1) programs + init (this script) → 2) TEST ownership return
# ON DEVNET → 3) only then --transfer-igp / --set-beneficiary / --seed.
#
#   bash deploy/solana-deploy.sh            # deploy of the .so + init + domain
#   bash deploy/solana-deploy.sh finalize   # transfer IGP + beneficiary + seed
#
# Keypair: current IGP owner (BirXd4…Ef1j).
# Cost: ~1.29 SOL of rent for pod.so (vault+governor FUSED into one program) +
# ~0.09 SOL of init/top-up. Rent recoverable via `solana program close`.
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

command -v solana >/dev/null || { echo "solana CLI missing"; exit 1; }
say "signer: $(solana address -k "$KEYPAIR") · balance: $(solana balance -k "$KEYPAIR" -u "$RPC")"

# symlink so init uses the oracle-agent node_modules
[ -e "$ROOT/deploy/node_modules" ] || ln -s ../oracle-agent/node_modules "$ROOT/deploy/node_modules"

if [ "${1:-}" = "finalize" ]; then
  POD=$(get_state POD_ID)
  [ -n "$POD" ] || { echo "❌ run the deploy first"; exit 1; }
  say "FINALIZE: transfer IGP + beneficiary + seed (did you TEST on devnet? ctrl-c if not)"
  sleep 5
  SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$POD" --transfer-igp --set-beneficiary --seed
  echo; echo "⚠️ LAST SECURITY STEP (manual, once the multisig exists):"
  echo "   solana program set-upgrade-authority $POD --new-upgrade-authority <MULTISIG> -k $KEYPAIR -u $RPC"
  exit 0
fi

say "1/2 build-sbf (uses the already-built pod.so if present)"
[ -f "$ROOT/svm/target/deploy/pod.so" ] || (cd "$ROOT/svm" && cargo build-sbf)
ls -la "$ROOT"/svm/target/deploy/pod.so

# --max-len = EXACT size of the .so → HALF the rent (without the 2x upgrade
# headroom). Trade-off: upgrade only for a binary <= current size; a larger upgrade requires
# close+redeploy. Since the upgrade authority goes to a multisig, this is acceptable.
deploy_prog() {  # $1=file.so $2=state_key $3=step
  local so="$ROOT/svm/target/deploy/$1" key="$2"
  done_step "$key" && { echo "$(get_state "$key")"; return; }
  say "$3 deploy $1 (--max-len $(stat -c%s "$so"))"
  local out
  out=$(solana program deploy "$so" --max-len "$(stat -c%s "$so")" -k "$KEYPAIR" -u "$RPC" --output json)
  mark "$key" "$(echo "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["programId"])')"
}
# pod.so = vault + governor FUSED into a single program (the solana+borsh runtime,
# ~90% of the bytes, is paid ONCE): rent 1.29 SOL vs 1.9 for the two separately.
deploy_prog pod.so POD_ID "2/2" >/dev/null
echo "✓ pod program (vault+governor): $(get_state POD_ID)"

# VAULT_ONLY=1 → initializes ONLY the vault module and points the IGP beneficiary
# directly (without governor). The price stays with the IGP owner until Phase 4b.
if [ "${VAULT_ONLY:-0}" = "1" ]; then
  say "init (ONLY vault module — VAULT_ONLY)"
  SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$(get_state POD_ID)" --vault-only ${VAULT_ONLY_FLAGS:-}
  echo; echo "governor (already in the binary) is left for Phase 4b: run without VAULT_ONLY to initialize it."
  exit 0
fi

say "init (vault + governor + domain 132556 + top-up of the config PDA)"
SOLANA_KEYPAIR="$KEYPAIR" SOLANA_RPC="$RPC" node "$ROOT/deploy/solana-init.mjs" "$(get_state POD_ID)"

say "NEXT STEPS"
echo "1. TEST ON DEVNET the ownership return (spec §08 — mandatory):"
echo "   deploy the same programs on devnet + TransferIgpOwnership round trip"
echo "2. bash deploy/solana-deploy.sh finalize   # transfer + beneficiary + seed"
echo "3. upgrade authority → multisig (command printed in finalize)"
