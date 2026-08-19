#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault RECIBO TRUSTLESS (EVM) — deploy + config LOCAL
#
#   uso:  PRIVATE_KEY=0x... bash deploy/evm-vault-receipt.sh bsc
#
# Deploya o vault com send_receipt/handle (o v2-atestação vira deprecado),
# aponta o IGP.beneficiary p/ ele e configura o corredor <chain>↔TC:
#   - operatorAddress[0][local]  = executor local  (alimenta reverse-lookup)
#   - operatorAddress[0][132556] = endereço do operador no TC (registro)
#   - remoteRouter[132556]       = vault do TC (canônico 32B) — confia/despacha
#   - remoteReward[132556]       = taxa real de origem BSC→TC (BNB)
# Idempotente via deploy/evm-receipt-<chain>.state. NADA disso na VPS.
# =============================================================================
set -euo pipefail
CHAIN="${1:?uso: evm-vault-receipt.sh bsc|ethereum}"
: "${PRIVATE_KEY:?exporte PRIVATE_KEY=0x...}"

TC_DOMAIN=132556
TC_VAULT_HEX32="0x402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579"
OPERADOR_TC="terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp"

case "$CHAIN" in
  bsc)
    RPC="${RPC_OVERRIDE:-https://bsc-dataseed.bnbchain.org}"; LEGACY="--legacy"
    LOCAL_DOMAIN=56
    EXPECTED_OWNER="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    OPERATOR_LOCAL="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    MAILBOX="0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4"
    IGP="0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923"
    ORACLE="0x7dE950f8F0a037783989a6BE84B3620916552306"
    WARP_ISM="0xa82087B8eea0394B1476f716B91c10531025Ef42"   # ISM do warp p/ TC→BSC
    REWARD_WEI="50000000000000"; WINDOW_BLOCKS="1600000"
    ;;
  ethereum)
    RPC="${RPC_OVERRIDE:-https://ethereum-rpc.publicnode.com}"; LEGACY=""
    LOCAL_DOMAIN=1
    EXPECTED_OWNER="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    OPERATOR_LOCAL="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    MAILBOX="0xc005dc82818d67AF737725bD4bf75435d065D239"
    IGP="0x9650F1f8DB492750323172145e67Df4e89E964Aa"
    ORACLE="0x3987cCE8f08037EBF93Ef3a934753540A94196cE"
    WARP_ISM="0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b"   # ISM do warp p/ TC→ETH
    REWARD_WEI="400000000000000"; WINDOW_BLOCKS="100800"
    ;;
  *) echo "chain desconhecida: $CHAIN"; exit 1;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE="$ROOT/deploy/evm-receipt-$CHAIN.state"; touch "$STATE"
mark(){ echo "$1=$2" >> "$STATE"; }; done_step(){ grep -q "^$1=" "$STATE"; }
get_state(){ grep "^$1=" "$STATE" | tail -1 | cut -d= -f2; }
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }
SEND(){ cast send $LEGACY --rpc-url "$RPC" --private-key "$PRIVATE_KEY" "$@"; }

SIGNER=$(cast wallet address --private-key "$PRIVATE_KEY")
say "signer: $SIGNER (chain: $CHAIN, dom $LOCAL_DOMAIN)"
[ "${SIGNER,,}" = "${EXPECTED_OWNER,,}" ] || { echo "❌ signer != owner ($EXPECTED_OWNER)"; exit 1; }
cd "$ROOT/evm"

if ! done_step VAULT; then
  say "1/7 deploy do vault (recibo) — construtor c/ localDomain=$LOCAL_DOMAIN"
  nonce=$(cast nonce --rpc-url "$RPC" "$SIGNER")
  predicted=$(cast compute-address "$SIGNER" --nonce "$nonce" --rpc-url "$RPC" | awk '{print $NF}')
  forge create src/RelayerRewardVault.sol:RelayerRewardVault \
    --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast $LEGACY \
    --constructor-args "$MAILBOX" "$SIGNER" "$REWARD_WEI" "$WINDOW_BLOCKS" "$LOCAL_DOMAIN" >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    [ "$(cast code --rpc-url "$RPC" "$predicted" | wc -c)" -gt 3 ] && { mark VAULT "$predicted"; break; }; sleep 4
  done
  done_step VAULT || { echo "❌ deploy não confirmou em $predicted"; exit 1; }
fi
V=$(get_state VAULT); echo "✓ vault: $V"

done_step BENEF || { say "2/7 igp.setBeneficiary(vault)"; SEND "$IGP" "setBeneficiary(address)" "$V" >/dev/null; mark BENEF ok; }
echo "✓ beneficiary: $(cast call --rpc-url "$RPC" "$IGP" 'beneficiary()(address)')"

done_step OP_LOCAL || { say "3/7 operatorAddress[0][$LOCAL_DOMAIN] = $OPERATOR_LOCAL (reverse-lookup)"; \
  SEND "$V" "setOperatorAddress(uint32,uint32,string)" 0 "$LOCAL_DOMAIN" "$OPERATOR_LOCAL" >/dev/null; mark OP_LOCAL ok; }
done_step OP_TC || { say "4/7 operatorAddress[0][132556] = $OPERADOR_TC (registro)"; \
  SEND "$V" "setOperatorAddress(uint32,uint32,string)" 0 "$TC_DOMAIN" "$OPERADOR_TC" >/dev/null; mark OP_TC ok; }
done_step ROUTER || { say "5/7 remoteRouter[132556] = vault do TC (canônico)"; \
  SEND "$V" "setRemoteRouter(uint32,bytes32)" "$TC_DOMAIN" "$TC_VAULT_HEX32" >/dev/null; mark ROUTER ok; }
done_step ISM || { say "5b/7 setIsm = $WARP_ISM (o mesmo ISM do warp — valida recibos vindos do TC)"; \
  SEND "$V" "setIsm(address)" "$WARP_ISM" >/dev/null; mark ISM ok; }

if ! done_step REWARD; then
  say "6/7 remoteReward[132556] = taxa real BSC→TC (fórmula do IGP custom)"
  OVERHEAD=$(cast call --rpc-url "$RPC" "$IGP" "gasOverhead()(uint96)" | awk '{print $1}')
  vals=$(cast call --rpc-url "$RPC" "$ORACLE" "getExchangeRateAndGasPrice(uint32)(uint128,uint128)" "$TC_DOMAIN" | awk '{print $1}' | paste -sd' ' -)
  read -r RATE GASP <<< "$vals"
  Q=$(python3 -c "print((50000 + int('$OVERHEAD')) * int('$GASP') * int('$RATE') // 10**10)")
  echo "  overhead=$OVERHEAD rate=$RATE gas=$GASP → $Q wei"
  SEND "$V" "setRemoteReward(uint32,uint256)" "$TC_DOMAIN" "$Q" >/dev/null; mark REWARD "$Q"
fi

say "7/7 VERIFICAÇÃO"
echo "localDomain:  $(cast call --rpc-url "$RPC" "$V" 'localDomain()(uint32)')"
echo "router[TC]:   $(cast call --rpc-url "$RPC" "$V" 'remoteRouter(uint32)(bytes32)' $TC_DOMAIN)"
echo "reward[TC]:   $(cast call --rpc-url "$RPC" "$V" 'remoteReward(uint32)(uint256)' $TC_DOMAIN)"
echo "op0/local:    $(cast call --rpc-url "$RPC" "$V" 'operatorOfLocal(address)(bool,uint32)' $OPERATOR_LOCAL)"
echo "ism:          $(cast call --rpc-url "$RPC" "$V" 'interchainSecurityModule()(address)')"
say "VAULT RECIBO ($CHAIN) NO AR: $V"
echo "➡️  agora rode:  BSC_VAULT=$V bash deploy/tc-migrate-vault-receipt.sh"
echo "   (registra este vault como router no TC + semear os pools p/ pagar)"
