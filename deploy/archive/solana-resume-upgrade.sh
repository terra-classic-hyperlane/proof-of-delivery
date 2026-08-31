#!/usr/bin/env bash
# =============================================================================
# RESUMES the pod upgrade from the partial buffer (when writes drop due to an
# unstable RPC). Reuses the already-funded buffer — does NOT create another one
# (saves ~1.6 SOL).
#
# BEFORE running, recover the ephemeral buffer keypair with the 12-word seed
# that the deploy printed ("To recover... following 12-word seed phrase"):
#
#   solana-keygen recover -o /tmp/pod-buffer.json 'prompt://'
#   # paste the seed:  flat stem velvet fun come crack dove parade baby turkey scene shine
#   # (leave the passphrase empty — just ENTER)
#
# Check that the recovered key == the printed buffer (EtBMW…):
#   solana-keygen pubkey /tmp/pod-buffer.json
#
#   usage:  bash deploy/solana-resume-upgrade.sh
#           BUFFER_KP=/tmp/pod-buffer.json  (override)
# =============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POD=2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj
SO="$ROOT/svm/target/deploy/pod.so"
RPC="${RPC:-https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42}"
KEYPAIR="${KEYPAIR:-/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json}"
BUFFER_KP="${BUFFER_KP:-/tmp/pod-buffer.json}"
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

[ -f "$BUFFER_KP" ] || { echo "❌ $BUFFER_KP does not exist — recover the buffer keypair (see the header)"; exit 1; }
BUF=$(solana-keygen pubkey "$BUFFER_KP")
say "resuming upgrade — buffer $BUF"
echo "buffer balance: $(solana balance "$BUF" -u "$RPC" | awk '{print $1}') SOL"
echo "authority:    $(solana balance "$KEYPAIR" -u "$RPC" | awk '{print $1}') SOL"

# --buffer reuses the existing buffer: writes only the missing chunks and does the swap.
# If it drops again due to RPC, YOU CAN RUN IT AGAIN — it is idempotent (only resends what is missing).
solana program deploy "$SO" \
  --program-id "$POD" \
  --buffer "$BUFFER_KP" \
  --upgrade-authority "$KEYPAIR" \
  -u "$RPC"

say "verification"
solana program show "$POD" -u "$RPC" | grep -E "Program Id|Authority|Last Deployed|Data Length"
echo
echo "✅ UPGRADE DONE. NOW THE MANDATORY STEP 2:"
echo "   node deploy/rrv-migrate-applied-base.mjs"
