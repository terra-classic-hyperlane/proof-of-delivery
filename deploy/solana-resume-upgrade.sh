#!/usr/bin/env bash
# =============================================================================
# RETOMA o upgrade do pod a partir do buffer parcial (quando as escritas caem por
# RPC instável). Reusa o buffer já financiado — NÃO cria outro (economiza ~1,6 SOL).
#
# ANTES de rodar, recupere a keypair efêmera do buffer com a seed de 12 palavras
# que o deploy imprimiu ("To recover... following 12-word seed phrase"):
#
#   solana-keygen recover -o /tmp/pod-buffer.json 'prompt://'
#   # cole a seed:  flat stem velvet fun come crack dove parade baby turkey scene shine
#   # (deixe a passphrase vazia — só ENTER)
#
# Confira que a chave recuperada == o buffer impresso (EtBMW…):
#   solana-keygen pubkey /tmp/pod-buffer.json
#
#   uso:  bash deploy/solana-resume-upgrade.sh
#         BUFFER_KP=/tmp/pod-buffer.json  (override)
# =============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POD=2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj
SO="$ROOT/svm/target/deploy/pod.so"
RPC="${RPC:-https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42}"
KEYPAIR="${KEYPAIR:-/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json}"
BUFFER_KP="${BUFFER_KP:-/tmp/pod-buffer.json}"
say(){ printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

[ -f "$BUFFER_KP" ] || { echo "❌ $BUFFER_KP não existe — recupere a keypair do buffer (veja o cabeçalho)"; exit 1; }
BUF=$(solana-keygen pubkey "$BUFFER_KP")
say "retomando upgrade — buffer $BUF"
echo "buffer saldo: $(solana balance "$BUF" -u "$RPC" | awk '{print $1}') SOL"
echo "authority:    $(solana balance "$KEYPAIR" -u "$RPC" | awk '{print $1}') SOL"

# --buffer reusa o buffer existente: escreve só os chunks faltantes e faz o swap.
# Se cair de novo por RPC, PODE RODAR OUTRA VEZ — é idempotente (só reenvia o que falta).
solana program deploy "$SO" \
  --program-id "$POD" \
  --buffer "$BUFFER_KP" \
  --upgrade-authority "$KEYPAIR" \
  -u "$RPC"

say "verificação"
solana program show "$POD" -u "$RPC" | grep -E "Program Id|Authority|Last Deployed|Data Length"
echo
echo "✅ UPGRADE FEITO. AGORA O PASSO 2 OBRIGATÓRIO:"
echo "   node deploy/rrv-migrate-applied-base.mjs"
