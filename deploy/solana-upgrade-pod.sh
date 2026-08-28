#!/usr/bin/env bash
# =============================================================================
# UPGRADE of the `pod` program on Solana mainnet (rrv with replay bitmap + close/
# refund of the epoch rent). Done with the BirXd4Q upgrade authority (local keypair).
#
# ⚠️ THIRD-PARTY MONEY. MANDATORY sequence (do not invert):
#   1. this script (extend + upgrade)  →  2. rrv-migrate-applied-base.mjs
# Without step 2, the migrated Config ends up with applied_base=0 and EVERY epoch
# submission is rejected (ERR_EPOCH_TOO_FUTURE) — the TC→Solana reporter stops.
#
# The upgrade does NOT touch the commission pool (stays in the Config account) nor
# the credits; it only swaps the bytecode. Reversible: re-upgrade with the previous
# binary (git checkout of the parent commit + rebuild) if something goes wrong.
#
#   usage:  bash deploy/solana-upgrade-pod.sh
#           RPC=https://... KEYPAIR=/path/BirXd4Q.json  (overrides)
# =============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POD=2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj
SO="$ROOT/svm/target/deploy/pod.so"
RPC="${RPC:-https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42}"
KEYPAIR="${KEYPAIR:-/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json}"
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

[ -f "$SO" ] || { echo "❌ $SO does not exist — run 'cd svm && cargo build-sbf'"; exit 1; }
[ -f "$KEYPAIR" ] || { echo "❌ keypair not found: $KEYPAIR"; exit 1; }

say "0/3 pre-check"
echo "pod.so:        $SO ($(stat -c%s "$SO") bytes)  sha256 $(sha256sum "$SO" | cut -c1-16)…"
AUTH=$(solana-keygen pubkey "$KEYPAIR")
echo "authority:     $AUTH  (expected BirXd4Q…)"
[ "$AUTH" = "BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j" ] || { echo "❌ keypair is not the upgrade authority"; exit 1; }
BAL=$(solana balance "$KEYPAIR" -u "$RPC" | awk '{print $1}')
echo "authority balance: $BAL SOL (upgrade needs ~1.6 SOL temporary for buffer + extend)"
CUR=$(solana program show "$POD" -u "$RPC" 2>/dev/null | awk '/Data Length/{print $3}')
echo "on-chain size: ${CUR:-?} bytes | new: $(stat -c%s "$SO") bytes"

read -rp $'\n⚠️  Confirm the pod UPGRADE on MAINNET? (type: UPGRADE) ' OK
[ "$OK" = "UPGRADE" ] || { echo "aborted."; exit 1; }

say "1/3 extend of programData (+10240 bytes — minimum required by the loader; the new binary is bigger)"
solana program extend "$POD" 10240 -u "$RPC" -k "$KEYPAIR"

say "2/3 deploy of the upgrade (buffer + atomic swap — the old program runs until the swap)"
solana program deploy "$SO" --program-id "$POD" -u "$RPC" -k "$KEYPAIR" --upgrade-authority "$KEYPAIR"

say "3/3 verification"
solana program show "$POD" -u "$RPC" | grep -E "Program Id|Authority|Last Deployed|Data Length"
echo
echo "✅ UPGRADE DONE. NEXT MANDATORY STEP (do not skip):"
echo "   node deploy/rrv-migrate-applied-base.mjs"
echo "   (sets applied_base to the current epoch − 256; without it the reporter stops)"
