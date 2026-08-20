#!/usr/bin/env bash
# =============================================================================
# UPGRADE do programa `pod` na Solana mainnet (rrv com bitmap de replay + close/
# refund do rent da época). Feito com a upgrade authority BirXd4Q (keypair local).
#
# ⚠️ DINHEIRO DE TERCEIROS. Sequência OBRIGATÓRIA (não inverter):
#   1. este script (extend + upgrade)  →  2. rrv-migrate-applied-base.mjs
# Sem o passo 2, o Config migrado fica com applied_base=0 e TODA submissão de
# época é rejeitada (ERR_EPOCH_TOO_FUTURE) — o reporter TC→Solana para.
#
# O upgrade NÃO toca no pool de comissões (fica na conta do Config) nem nos
# créditos; só troca o bytecode. Reversível: re-upgrade com o binário anterior
# (git checkout do commit pai + rebuild) se algo der errado.
#
#   uso:  bash deploy/solana-upgrade-pod.sh
#         RPC=https://... KEYPAIR=/caminho/BirXd4Q.json  (overrides)
# =============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POD=2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj
SO="$ROOT/svm/target/deploy/pod.so"
RPC="${RPC:-https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42}"
KEYPAIR="${KEYPAIR:-/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json}"
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

[ -f "$SO" ] || { echo "❌ $SO não existe — rode 'cd svm && cargo build-sbf'"; exit 1; }
[ -f "$KEYPAIR" ] || { echo "❌ keypair não encontrado: $KEYPAIR"; exit 1; }

say "0/3 pré-checagem"
echo "pod.so:        $SO ($(stat -c%s "$SO") bytes)  sha256 $(sha256sum "$SO" | cut -c1-16)…"
AUTH=$(solana-keygen pubkey "$KEYPAIR")
echo "authority:     $AUTH  (esperado BirXd4Q…)"
[ "$AUTH" = "BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j" ] || { echo "❌ keypair não é a upgrade authority"; exit 1; }
BAL=$(solana balance "$KEYPAIR" -u "$RPC" | awk '{print $1}')
echo "saldo authority: $BAL SOL (upgrade precisa ~1,6 SOL temporário de buffer + extend)"
CUR=$(solana program show "$POD" -u "$RPC" 2>/dev/null | awk '/Data Length/{print $3}')
echo "tamanho on-chain: ${CUR:-?} bytes | novo: $(stat -c%s "$SO") bytes"

read -rp $'\n⚠️  Confirma o UPGRADE do pod no MAINNET? (digite: UPGRADE) ' OK
[ "$OK" = "UPGRADE" ] || { echo "abortado."; exit 1; }

say "1/3 extend do programData (+8192 bytes de folga; o novo binário é maior)"
solana program extend "$POD" 8192 -u "$RPC" -k "$KEYPAIR" || echo "  (se já tiver espaço, o extend pode falhar — segue pro deploy)"

say "2/3 deploy do upgrade (buffer + swap atômico — o programa antigo roda até o swap)"
solana program deploy "$SO" --program-id "$POD" -u "$RPC" -k "$KEYPAIR" --upgrade-authority "$KEYPAIR"

say "3/3 verificação"
solana program show "$POD" -u "$RPC" | grep -E "Program Id|Authority|Last Deployed|Data Length"
echo
echo "✅ UPGRADE FEITO. PRÓXIMO PASSO OBRIGATÓRIO (não pule):"
echo "   node deploy/rrv-migrate-applied-base.mjs"
echo "   (seta applied_base p/ a época atual − 256; sem isso o reporter para)"
