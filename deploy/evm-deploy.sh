#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Deploy Phase 3 (EVM): BSC or Ethereum
#
#   usage:  PRIVATE_KEY=0x... bash deploy/evm-deploy.sh bsc|ethereum
#
# PRIVATE_KEY must be that of the CURRENT OWNER of the oracle/IGP of the chosen chain
# (BSC: 0x8f085bAD…5291 · ETH: 0xEF818120…00ae) — the script checks and aborts
# if it is not, because transferOwnership/setBeneficiary would fail.
#
# Addresses discovered ON-CHAIN on 2026-08-18 (from the routers wired
# into the TC warp) — see docs/PROPOSAL-PARAMETERS.md. Oracle bounds anchored
# to the CURRENT values (÷3 · ×3): the oracle is the custom TerraClassicOracle
# (setRemoteGasData flat, selector 0x666af432).
# =============================================================================
set -euo pipefail
CHAIN="${1:?usage: evm-deploy.sh bsc|ethereum}"
: "${PRIVATE_KEY:?export PRIVATE_KEY=0x...}"

case "$CHAIN" in
  bsc)
    # publicnode returns 403 "archive requires token" on sends — use official dataseed.
    # override: RPC_OVERRIDE=https://... bash evm-deploy.sh bsc
    RPC="${RPC_OVERRIDE:-https://bsc-dataseed.bnbchain.org}"
    EXPECTED_OWNER="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    MAILBOX="0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4"
    IGP="0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923"
    ORACLE="0x7dE950f8F0a037783989a6BE84B3620916552306"
    REWARD_WEI="50000000000000"        # 0.00005 BNB (§2 of the proposal)
    WINDOW_BLOCKS="1600000"            # ~14d (block ~0.75s — confirm)
    SEED_WEI="5000000000000000"        # 0.005 BNB (100× fee)
    ;;
  ethereum)
    RPC="${RPC_OVERRIDE:-https://ethereum-rpc.publicnode.com}"
    EXPECTED_OWNER="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    MAILBOX="0xc005dc82818d67AF737725bD4bf75435d065D239"
    IGP="0x9650F1f8DB492750323172145e67Df4e89E964Aa"
    ORACLE="0x3987cCE8f08037EBF93Ef3a934753540A94196cE"
    REWARD_WEI="400000000000000"       # 0.0004 ETH
    WINDOW_BLOCKS="100800"             # ~14d @12s
    SEED_WEI="40000000000000000"       # 0.04 ETH (100× fee)
    ;;
  *) echo "unknown chain: $CHAIN"; exit 1;;
esac

TC_DOMAIN=132556
EPOCH_SECS=21600
DELTA_BPS=2000
# --legacy only on BSC (does not support EIP-1559 well); ETH uses dynamic (avoids underprice).
LEGACY=""; [ "$CHAIN" = "bsc" ] && LEGACY="--legacy"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE="$ROOT/deploy/evm-$CHAIN.state"
touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }
done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

SIGNER=$(cast wallet address --private-key "$PRIVATE_KEY")
say "signer: $SIGNER (chain: $CHAIN)"
[ "${SIGNER,,}" = "${EXPECTED_OWNER,,}" ] || { echo "❌ signer is not the oracle/IGP owner ($EXPECTED_OWNER)"; exit 1; }

CUR_ORACLE_OWNER=$(cast call --rpc-url "$RPC" "$ORACLE" "owner()(address)")
[ "${CUR_ORACLE_OWNER,,}" = "${SIGNER,,}" ] || { echo "❌ current oracle owner is $CUR_ORACLE_OWNER"; exit 1; }
echo "✓ signer is oracle owner"

cd "$ROOT/evm"

# forge create on BSC frequently does NOT confirm in time even with the tx included.
# fc(): computes the address via nonce BEFORE sending, sends with --legacy, and
# confirms by reading the code at the predicted address (idempotent and timeout-proof).
fc() {  # $1=contract $2=step_key  ...rest = constructor-args
  local contract="$1" key="$2"; shift 2
  if done_step "$key"; then echo "$(get_state "$key")"; return; fi
  local nonce predicted
  nonce=$(cast nonce --rpc-url "$RPC" "$SIGNER")
  predicted=$(cast compute-address "$SIGNER" --nonce "$nonce" --rpc-url "$RPC" | awk '{print $NF}')
  forge create "$contract" --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast $LEGACY \
    --constructor-args "$@" >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    [ "$(cast code --rpc-url "$RPC" "$predicted" | wc -c)" -gt 3 ] && { mark "$key" "$predicted"; echo "$predicted"; return; }
    sleep 4
  done
  echo "❌ $contract did not confirm at $predicted (nonce $nonce)"; exit 1
}

if ! done_step VAULT; then
  say "1/6 deploy RelayerRewardVault (reward=$REWARD_WEI wei · window=$WINDOW_BLOCKS blocks)"
  fc "src/RelayerRewardVault.sol:RelayerRewardVault" VAULT "$MAILBOX" "$SIGNER" "$REWARD_WEI" "$WINDOW_BLOCKS" >/dev/null
fi
VAULT=$(get_state VAULT); echo "✓ vault: $VAULT"

# operators: signer + (optional) OPERATOR2 via env; quorum follows (docs/OPERATORS.md)
OPS_ARG="[$SIGNER]"; Q=1
if [ -n "${OPERATOR2:-}" ]; then OPS_ARG="[$SIGNER,$OPERATOR2]"; Q=${QUORUM:-2}; fi
if ! done_step GOV; then
  say "2/6 deploy GasOracleGovernor (operators: $OPS_ARG · quorum $Q · epoch 6h · delta 20%)"
  fc "src/GasOracleGovernor.sol:GasOracleGovernor" GOV "$ORACLE" "$SIGNER" "$OPS_ARG" "$Q" "$EPOCH_SECS" "$DELTA_BPS" >/dev/null
fi
GOV=$(get_state GOV); echo "✓ governor: $GOV"

if ! done_step BOUNDS; then
  say "3/6 setBounds(dom $TC_DOMAIN) — bounds DERIVED from the oracle in production NOW (current ÷3 · ×3)"
  vals=$(cast call --rpc-url "$RPC" "$ORACLE" "getExchangeRateAndGasPrice(uint32)(uint128,uint128)" "$TC_DOMAIN" \
    | tr -d '[]' | awk '{print $1}' | paste -sd' ' -)
  read -r CUR_RATE CUR_GAS <<< "$vals"
  [ -n "$CUR_RATE" ] && [ "$CUR_RATE" != "0" ] || { echo "❌ oracle has no current value for $TC_DOMAIN"; exit 1; }
  MIN_RATE=$((CUR_RATE/3)); MAX_RATE=$((CUR_RATE*3))
  MIN_GAS=$((CUR_GAS/3));   MAX_GAS=$((CUR_GAS*3))
  echo "  current read from oracle: rate=$CUR_RATE gas=$CUR_GAS → bounds [$MIN_RATE·$MAX_RATE] [$MIN_GAS·$MAX_GAS]"
  cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$GOV" \
    "setBounds(uint32,(uint128,uint128,uint128,uint128,bool))" \
    "$TC_DOMAIN" "($MIN_RATE,$MAX_RATE,$MIN_GAS,$MAX_GAS,true)" >/dev/null
  mark BOUNDS ok
fi
echo "✓ bounds set"

if ! done_step ORACLE_OWNER; then
  say "4/6 oracle.transferOwnership(governor)  ⚠️ SINGLE step — address checked 3×"
  [ "$(cast call --rpc-url "$RPC" "$GOV" "oracle()(address)")" = "$ORACLE" ] || { echo "❌ governor does not point to this oracle"; exit 1; }
  cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$ORACLE" \
    "transferOwnership(address)" "$GOV" >/dev/null
  mark ORACLE_OWNER ok
fi
echo "✓ oracle under the governor: $(cast call --rpc-url "$RPC" "$ORACLE" 'owner()(address)')"

if ! done_step BENEFICIARY; then
  say "5/6 igp.setBeneficiary(vault)"
  cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$IGP" \
    "setBeneficiary(address)" "$VAULT" >/dev/null
  mark BENEFICIARY ok
fi
echo "✓ beneficiary: $(cast call --rpc-url "$RPC" "$IGP" 'beneficiary()(address)')"

# Seed: only if there is comfortable balance (leaves margin for gas). SEED_WEI=0 skips.
# You can seed later with: cast send --legacy $VAULT --value <wei> --private-key ...
if ! done_step SEED && [ "${SEED_WEI:-0}" != "0" ]; then
  BAL=$(cast balance --rpc-url "$RPC" "$SIGNER")
  if python3 -c "import sys; sys.exit(0 if int('$BAL') > int('$SEED_WEI')*3 else 1)"; then
    say "6/6 pool seed ($SEED_WEI wei)"
    cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$VAULT" --value "$SEED_WEI" >/dev/null
    mark SEED ok
  else
    echo "⚠️ 6/6 seed SKIPPED — balance ($BAL wei) too low to seed $SEED_WEI + gas."
    echo "   seed later: cast send --legacy $VAULT --value $SEED_WEI --private-key <PK> --rpc-url $RPC"
  fi
fi

say "VERIFICATION"
echo "vault.claimsPayable: $(cast call --rpc-url "$RPC" "$VAULT" 'claimsPayable()(uint256)')"
echo "oracle data(132556): $(cast call --rpc-url "$RPC" "$ORACLE" 'getExchangeRateAndGasPrice(uint32)(uint128,uint128)' $TC_DOMAIN | tr '\n' ' ')"
echo "governor.currentEpoch: $(cast call --rpc-url "$RPC" "$GOV" 'currentEpoch()(uint256)')"
say "PHASE 3 ($CHAIN) COMPLETE 🎉  handoff to multisig: §8 of the proposal"
