#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault v2 EVM (ClaimRemote) — LOCAL deploy
#
#   usage:  PRIVATE_KEY=0x... bash deploy/evm-vault-v2.sh bsc|ethereum
#
# The EVM v1 vaults are not migratable (no proxy) and the pools are zeroed, so
# v2 is a NEW deploy + igp.setBeneficiary(v2). Remote config: attester =
# owner, quorum 1, binding (owner, dom 132556) → terra1run9wz… and reward =
# REAL IGP quote for a transfer to the TC (what the user pays as fee).
# Idempotent via deploy/evm-v2-<chain>.state. NONE of this runs on the VPS.
# =============================================================================
set -euo pipefail
CHAIN="${1:?usage: evm-vault-v2.sh bsc|ethereum}"
: "${PRIVATE_KEY:?export PRIVATE_KEY=0x...}"

case "$CHAIN" in
  bsc)
    RPC="${RPC_OVERRIDE:-https://bsc-dataseed.bnbchain.org}"
    LEGACY="--legacy"
    EXPECTED_OWNER="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    MAILBOX="0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4"
    IGP="0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923"
    ORACLE="0x7dE950f8F0a037783989a6BE84B3620916552306"
    REWARD_WEI="50000000000000"   # LOCAL fee per delivery (same as v1)
    WINDOW_BLOCKS="1600000"
    ;;
  ethereum)
    RPC="${RPC_OVERRIDE:-https://ethereum-rpc.publicnode.com}"
    LEGACY=""
    EXPECTED_OWNER="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    MAILBOX="0xc005dc82818d67AF737725bD4bf75435d065D239"
    IGP="0x9650F1f8DB492750323172145e67Df4e89E964Aa"
    ORACLE="0x3987cCE8f08037EBF93Ef3a934753540A94196cE"
    REWARD_WEI="400000000000000"
    WINDOW_BLOCKS="100800"
    ;;
  *) echo "unknown chain: $CHAIN"; exit 1;;
esac

TC_DOMAIN=132556
OPERADOR_TC="terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE="$ROOT/deploy/evm-v2-$CHAIN.state"
touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }
done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
SEND(){ cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$@"; }

SIGNER=$(cast wallet address --private-key "$PRIVATE_KEY")
say "signer: $SIGNER (chain: $CHAIN)"
[ "${SIGNER,,}" = "${EXPECTED_OWNER,,}" ] || { echo "❌ signer is not the owner ($EXPECTED_OWNER)"; exit 1; }

cd "$ROOT/evm"

if ! done_step VAULT2; then
  say "1/5 deploy of RelayerRewardVault v2"
  nonce=$(cast nonce --rpc-url "$RPC" "$SIGNER")
  predicted=$(cast compute-address "$SIGNER" --nonce "$nonce" --rpc-url "$RPC" | awk '{print $NF}')
  forge create src/RelayerRewardVault.sol:RelayerRewardVault \
    --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast $LEGACY \
    --constructor-args "$MAILBOX" "$SIGNER" "$REWARD_WEI" "$WINDOW_BLOCKS" >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    [ "$(cast code --rpc-url "$RPC" "$predicted" | wc -c)" -gt 3 ] && { mark VAULT2 "$predicted"; break; }
    sleep 4
  done
  done_step VAULT2 || { echo "❌ deploy did not confirm at $predicted"; exit 1; }
fi
V2=$(get_state VAULT2); echo "✓ vault v2: $V2"

if ! done_step BENEFICIARY; then
  say "2/5 igp.setBeneficiary(vault v2) — collection now lands in v2"
  SEND "$IGP" "setBeneficiary(address)" "$V2" >/dev/null
  mark BENEFICIARY ok
fi
echo "✓ beneficiary: $(cast call --rpc-url "$RPC" "$IGP" 'beneficiary()(address)')"

if ! done_step REMOTE_CFG; then
  say "3/5 attester (= owner) + quorum 1 + binding dom $TC_DOMAIN → $OPERADOR_TC"
  SEND "$V2" "setRemoteOperators(address[],uint256)" "[$SIGNER]" 1 >/dev/null
  SEND "$V2" "setRemoteBinding(address,uint32,string)" "$SIGNER" "$TC_DOMAIN" "$OPERADOR_TC" >/dev/null
  mark REMOTE_CFG ok
fi
echo "✓ remote config"

if ! done_step REMOTE_REWARD; then
  say "4/5 remote reward = REAL origin fee (TerraClassicIGPStandalone formula)"
  # the custom IGP does not expose a public quote: fee = (50k default + gasOverhead) × gasPrice × rate / 1e10
  OVERHEAD=$(cast call --rpc-url "$RPC" "$IGP" "gasOverhead()(uint96)" | awk '{print $1}')
  vals=$(cast call --rpc-url "$RPC" "$ORACLE" "getExchangeRateAndGasPrice(uint32)(uint128,uint128)" "$TC_DOMAIN" | awk '{print $1}' | paste -sd' ' -)
  read -r RATE GASP <<< "$vals"
  Q=$(python3 -c "print((50000 + int('$OVERHEAD')) * int('$GASP') * int('$RATE') // 10**10)")
  echo "  overhead=$OVERHEAD · rate=$RATE · gasPrice=$GASP → fee=$Q wei"
  [ -n "$Q" ] && [ "$Q" != "0" ] || { echo "❌ zero quote — check IGP/ORACLE"; exit 1; }
  SEND "$V2" "setRemoteReward(uint32,uint256)" "$TC_DOMAIN" "$Q" >/dev/null
  mark REMOTE_REWARD "$Q"
fi
echo "✓ remote reward: $(get_state REMOTE_REWARD) wei"

say "5/5 VERIFICATION"
echo "remoteQuorum: $(cast call --rpc-url "$RPC" "$V2" 'remoteQuorum()(uint256)')"
echo "binding: $(cast call --rpc-url "$RPC" "$V2" 'remoteBinding(address,uint32)(string)' "$SIGNER" "$TC_DOMAIN")"
echo "remoteReward: $(cast call --rpc-url "$RPC" "$V2" 'remoteReward(uint32)(uint256)' "$TC_DOMAIN")"
say "V2 ($CHAIN) LIVE — update the claim-agent (new vault) and seed the pool 🎉"
echo "OLD vault still exists (pool 0, no beneficiary) — merely deprecated."
