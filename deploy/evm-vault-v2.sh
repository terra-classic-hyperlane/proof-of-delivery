#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · Vault v2 EVM (ClaimRemote) — deploy LOCAL
#
#   uso:  PRIVATE_KEY=0x... bash deploy/evm-vault-v2.sh bsc|ethereum
#
# Os vaults EVM v1 não são migráveis (sem proxy) e os pools estão zerados, então
# a v2 é um deploy NOVO + igp.setBeneficiary(v2). Config remota: atestador =
# owner, quórum 1, vínculo (owner, dom 132556) → terra1run9wz… e recompensa =
# cotação REAL do IGP p/ um transfer até o TC (o que o usuário paga de taxa).
# Idempotente via deploy/evm-v2-<chain>.state. NADA disso roda na VPS.
# =============================================================================
set -euo pipefail
CHAIN="${1:?uso: evm-vault-v2.sh bsc|ethereum}"
: "${PRIVATE_KEY:?exporte PRIVATE_KEY=0x...}"

case "$CHAIN" in
  bsc)
    RPC="${RPC_OVERRIDE:-https://bsc-dataseed.bnbchain.org}"
    LEGACY="--legacy"
    EXPECTED_OWNER="0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"
    MAILBOX="0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4"
    IGP="0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923"
    WARP="0x3605D8946FC6F5A75d89d92173100F59743B5318"
    REWARD_WEI="50000000000000"   # tarifa por entrega LOCAL (igual à v1)
    WINDOW_BLOCKS="1600000"
    ;;
  ethereum)
    RPC="${RPC_OVERRIDE:-https://ethereum-rpc.publicnode.com}"
    LEGACY=""
    EXPECTED_OWNER="0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"
    MAILBOX="0xc005dc82818d67AF737725bD4bf75435d065D239"
    IGP="0x9650F1f8DB492750323172145e67Df4e89E964Aa"
    WARP="0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5"
    REWARD_WEI="400000000000000"
    WINDOW_BLOCKS="100800"
    ;;
  *) echo "chain desconhecida: $CHAIN"; exit 1;;
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
[ "${SIGNER,,}" = "${EXPECTED_OWNER,,}" ] || { echo "❌ signer não é o owner ($EXPECTED_OWNER)"; exit 1; }

cd "$ROOT/evm"

if ! done_step VAULT2; then
  say "1/5 deploy do RelayerRewardVault v2"
  nonce=$(cast nonce --rpc-url "$RPC" "$SIGNER")
  predicted=$(cast compute-address "$SIGNER" --nonce "$nonce" --rpc-url "$RPC" | awk '{print $NF}')
  forge create src/RelayerRewardVault.sol:RelayerRewardVault \
    --rpc-url "$RPC" --private-key "$PRIVATE_KEY" --broadcast $LEGACY \
    --constructor-args "$MAILBOX" "$SIGNER" "$REWARD_WEI" "$WINDOW_BLOCKS" >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    [ "$(cast code --rpc-url "$RPC" "$predicted" | wc -c)" -gt 3 ] && { mark VAULT2 "$predicted"; break; }
    sleep 4
  done
  done_step VAULT2 || { echo "❌ deploy não confirmou em $predicted"; exit 1; }
fi
V2=$(get_state VAULT2); echo "✓ vault v2: $V2"

if ! done_step BENEFICIARY; then
  say "2/5 igp.setBeneficiary(vault v2) — a arrecadação passa a cair no v2"
  SEND "$IGP" "setBeneficiary(address)" "$V2" >/dev/null
  mark BENEFICIARY ok
fi
echo "✓ beneficiary: $(cast call --rpc-url "$RPC" "$IGP" 'beneficiary()(address)')"

if ! done_step REMOTE_CFG; then
  say "3/5 atestador (= owner) + quórum 1 + vínculo dom $TC_DOMAIN → $OPERADOR_TC"
  SEND "$V2" "setRemoteOperators(address[],uint256)" "[$SIGNER]" 1 >/dev/null
  SEND "$V2" "setRemoteBinding(address,uint32,string)" "$SIGNER" "$TC_DOMAIN" "$OPERADOR_TC" >/dev/null
  mark REMOTE_CFG ok
fi
echo "✓ config remota"

if ! done_step REMOTE_REWARD; then
  say "4/5 recompensa remota = cotação REAL do IGP (taxa de origem de um transfer p/ o TC)"
  G=$(cast call --rpc-url "$RPC" "$WARP" "destinationGas(uint32)(uint256)" "$TC_DOMAIN" | awk '{print $1}')
  [ -z "$G" ] || [ "$G" = "0" ] && G=300000
  Q=$(cast call --rpc-url "$RPC" "$IGP" "quoteGasPayment(uint32,uint256)(uint256)" "$TC_DOMAIN" "$G" | awk '{print $1}')
  echo "  destinationGas=$G · quote=$Q wei"
  [ -n "$Q" ] && [ "$Q" != "0" ] || { echo "❌ cotação zero — confira IGP/WARP"; exit 1; }
  SEND "$V2" "setRemoteReward(uint32,uint256)" "$TC_DOMAIN" "$Q" >/dev/null
  mark REMOTE_REWARD "$Q"
fi
echo "✓ recompensa remota: $(get_state REMOTE_REWARD) wei"

say "5/5 VERIFICAÇÃO"
echo "remoteQuorum: $(cast call --rpc-url "$RPC" "$V2" 'remoteQuorum()(uint256)')"
echo "binding: $(cast call --rpc-url "$RPC" "$V2" 'remoteBinding(address,uint32)(string)' "$SIGNER" "$TC_DOMAIN")"
echo "remoteReward: $(cast call --rpc-url "$RPC" "$V2" 'remoteReward(uint32)(uint256)' "$TC_DOMAIN")"
say "V2 ($CHAIN) NO AR — atualize o claim-agent (vault novo) e semeie o pool 🎉"
echo "vault ANTIGO segue existindo (pool 0, sem beneficiário) — apenas deprecado."