#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Deploy Fase 3 (EVM): BSC ou Ethereum
#
#   uso:  PRIVATE_KEY=0x... bash deploy/evm-deploy.sh bsc|ethereum
#
# A PRIVATE_KEY deve ser a do OWNER ATUAL do oracle/IGP da chain escolhida
# (BSC: 0x8f085bAD…5291 · ETH: 0xEF818120…00ae) — o script confere e aborta
# se não for, porque transferOwnership/setBeneficiary falhariam.
#
# Endereços descobertos ON-CHAIN em 18/08/2026 (a partir dos routers enrolados
# no warp do TC) — ver docs/PARAMETROS_PROPOSTA.md. Faixas do oracle ancoradas
# nos valores VIGENTES (÷3 · ×3): o oracle é o TerraClassicOracle custom
# (setRemoteGasData flat, selector 0x666af432).
# =============================================================================
set -euo pipefail
CHAIN="${1:?uso: evm-deploy.sh bsc|ethereum}"
: "${PRIVATE_KEY:?exporte PRIVATE_KEY=0x...}"

case "$CHAIN" in
  bsc)
    RPC="https://bsc-rpc.publicnode.com"
    EXPECTED_OWNER="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    MAILBOX="0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4"
    IGP="0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923"
    ORACLE="0x7dE950f8F0a037783989a6BE84B3620916552306"
    REWARD_WEI="50000000000000"        # 0,00005 BNB (§2 da proposta)
    WINDOW_BLOCKS="1600000"            # ~14d (block ~0,75s — confirmar)
    SEED_WEI="5000000000000000"        # 0,005 BNB (100× tarifa)
    ;;
  ethereum)
    RPC="https://ethereum-rpc.publicnode.com"
    EXPECTED_OWNER="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    MAILBOX="0xc005dc82818d67AF737725bD4bf75435d065D239"
    IGP="0x9650F1f8DB492750323172145e67Df4e89E964Aa"
    ORACLE="0x3987cCE8f08037EBF93Ef3a934753540A94196cE"
    REWARD_WEI="400000000000000"       # 0,0004 ETH
    WINDOW_BLOCKS="100800"             # ~14d @12s
    SEED_WEI="40000000000000000"       # 0,04 ETH (100× tarifa)
    ;;
  *) echo "chain desconhecida: $CHAIN"; exit 1;;
esac

TC_DOMAIN=132556
EPOCH_SECS=21600
DELTA_BPS=2000
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE="$ROOT/deploy/evm-$CHAIN.state"
touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }
done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

SIGNER=$(cast wallet address --private-key "$PRIVATE_KEY")
say "signer: $SIGNER (chain: $CHAIN)"
[ "${SIGNER,,}" = "${EXPECTED_OWNER,,}" ] || { echo "❌ signer não é o owner do oracle/IGP ($EXPECTED_OWNER)"; exit 1; }

CUR_ORACLE_OWNER=$(cast call --rpc-url "$RPC" "$ORACLE" "owner()(address)")
[ "${CUR_ORACLE_OWNER,,}" = "${SIGNER,,}" ] || { echo "❌ oracle owner atual é $CUR_ORACLE_OWNER"; exit 1; }
echo "✓ signer é owner do oracle"

cd "$ROOT/evm"

if ! done_step VAULT; then
  say "1/6 deploy RelayerRewardVault (reward=$REWARD_WEI wei · janela=$WINDOW_BLOCKS blocos)"
  out=$(forge create src/RelayerRewardVault.sol:RelayerRewardVault \
        --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast \
        --constructor-args "$MAILBOX" "$SIGNER" "$REWARD_WEI" "$WINDOW_BLOCKS")
  addr=$(echo "$out" | grep -oE "Deployed to: 0x[0-9a-fA-F]{40}" | cut -d' ' -f3)
  mark VAULT "$addr"
fi
VAULT=$(get_state VAULT); echo "✓ vault: $VAULT"

# operadores: signer + (opcional) OPERATOR2 via env; quórum acompanha (docs/OPERADORES.md)
OPS_ARG="[$SIGNER]"; Q=1
if [ -n "${OPERATOR2:-}" ]; then OPS_ARG="[$SIGNER,$OPERATOR2]"; Q=${QUORUM:-2}; fi
if ! done_step GOV; then
  say "2/6 deploy GasOracleGovernor (operadores: $OPS_ARG · quórum $Q · época 6h · delta 20%)"
  out=$(forge create src/GasOracleGovernor.sol:GasOracleGovernor \
        --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast \
        --constructor-args "$ORACLE" "$SIGNER" "$OPS_ARG" "$Q" "$EPOCH_SECS" "$DELTA_BPS")
  addr=$(echo "$out" | grep -oE "Deployed to: 0x[0-9a-fA-F]{40}" | cut -d' ' -f3)
  mark GOV "$addr"
fi
GOV=$(get_state GOV); echo "✓ governor: $GOV"

if ! done_step BOUNDS; then
  say "3/6 setBounds(dom $TC_DOMAIN) — faixa DERIVADA do oracle em produção AGORA (vigente ÷3 · ×3)"
  vals=$(cast call --rpc-url "$RPC" "$ORACLE" "getExchangeRateAndGasPrice(uint32)(uint128,uint128)" "$TC_DOMAIN" \
    | tr -d '[]' | awk '{print $1}' | paste -sd' ' -)
  read -r CUR_RATE CUR_GAS <<< "$vals"
  [ -n "$CUR_RATE" ] && [ "$CUR_RATE" != "0" ] || { echo "❌ oracle sem valor vigente p/ $TC_DOMAIN"; exit 1; }
  MIN_RATE=$((CUR_RATE/3)); MAX_RATE=$((CUR_RATE*3))
  MIN_GAS=$((CUR_GAS/3));   MAX_GAS=$((CUR_GAS*3))
  echo "  vigente lido do oracle: rate=$CUR_RATE gas=$CUR_GAS → faixas [$MIN_RATE·$MAX_RATE] [$MIN_GAS·$MAX_GAS]"
  cast send --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$GOV" \
    "setBounds(uint32,(uint128,uint128,uint128,uint128,bool))" \
    "$TC_DOMAIN" "($MIN_RATE,$MAX_RATE,$MIN_GAS,$MAX_GAS,true)" >/dev/null
  mark BOUNDS ok
fi
echo "✓ faixas definidas"

if ! done_step ORACLE_OWNER; then
  say "4/6 oracle.transferOwnership(governor)  ⚠️ passo ÚNICO — endereço conferido 3×"
  [ "$(cast call --rpc-url "$RPC" "$GOV" "oracle()(address)")" = "$ORACLE" ] || { echo "❌ governor não aponta p/ este oracle"; exit 1; }
  cast send --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$ORACLE" \
    "transferOwnership(address)" "$GOV" >/dev/null
  mark ORACLE_OWNER ok
fi
echo "✓ oracle sob o governor: $(cast call --rpc-url "$RPC" "$ORACLE" 'owner()(address)')"

if ! done_step BENEFICIARY; then
  say "5/6 igp.setBeneficiary(vault)"
  cast send --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$IGP" \
    "setBeneficiary(address)" "$VAULT" >/dev/null
  mark BENEFICIARY ok
fi
echo "✓ beneficiary: $(cast call --rpc-url "$RPC" "$IGP" 'beneficiary()(address)')"

if ! done_step SEED; then
  say "6/6 semente do pool ($SEED_WEI wei)"
  cast send --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$VAULT" --value "$SEED_WEI" >/dev/null
  mark SEED ok
fi

say "VERIFICAÇÃO"
echo "vault.claimsPayable: $(cast call --rpc-url "$RPC" "$VAULT" 'claimsPayable()(uint256)')"
echo "oracle dados(132556): $(cast call --rpc-url "$RPC" "$ORACLE" 'getExchangeRateAndGasPrice(uint32)(uint128,uint128)' $TC_DOMAIN | tr '\n' ' ')"
echo "governor.currentEpoch: $(cast call --rpc-url "$RPC" "$GOV" 'currentEpoch()(uint256)')"
say "FASE 3 ($CHAIN) CONCLUÍDA 🎉  handoff p/ multisig: §8 da proposta"
