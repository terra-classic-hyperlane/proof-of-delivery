#!/usr/bin/env bash
# =============================================================================
# IGORFAKE (TC) — remove a rota OBSOLETA do domínio 1399811151 (Solana devnet)
#
# Pedido pelo revisor do hyperlane-registry PR #1559 (paulbalaji): o router do
# TC tem 4 rotas enroladas (1, 56, 1399811149 e 1399811151-devnet), mas o PR
# declara só as 3 de produção. Este script desenrola a devnet (route: null).
#
#   uso: bash deploy/tc-unenroll-devnet-route.sh
#
# ⚠️ Depois de rodar, REINICIE O RELAYER na VPS (mesma chave = sequence):
#     ssh root@31.97.91.4 'systemctl restart hyperlane-relayer'
# =============================================================================
set -euo pipefail
KEY="${KEY:-hyperlane-deploy}"; KEYRING="file"
NODE=https://rpc.terra-classic.hexxagon.io:443
WARP=terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel
OPERADOR=terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp
TX=(--from "$KEY" --keyring-backend "$KEYRING" --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna --chain-id columbus-5 --node "$NODE" -y --output json --broadcast-mode sync)

read -rs -p "Senha do keyring (chave $KEY): " PASS; echo
sign(){ printf '%s\n%s\n' "$PASS" "$PASS" | terrad "$@"; }
ADDR=$(sign keys show "$KEY" -a --keyring-backend "$KEYRING")
[ "$ADDR" = "$OPERADOR" ] || { echo "❌ chave $KEY = $ADDR, esperado $OPERADOR"; exit 1; }

echo "== rotas ANTES =="
terrad q wasm contract-state smart "$WARP" '{"router":{"list_routes":{}}}' --node "$NODE" --output json | python3 -m json.tool | grep -E "domain|route"

echo "== removendo rota do domínio 1399811151 (devnet) =="
H=$(sign tx wasm execute "$WARP" '{"router":{"set_route":{"set":{"domain":1399811151,"route":null}}}}' "${TX[@]}" | python3 -c 'import json,sys;print(json.load(sys.stdin)["txhash"])')
echo "tx: $H — aguardando…"
for i in $(seq 1 20); do R=$(terrad q tx "$H" --node "$NODE" --output json 2>/dev/null) && break; sleep 3; done
echo "$R" | python3 -c 'import json,sys;r=json.load(sys.stdin);assert r.get("code")==0, r.get("raw_log"); print("✓ confirmada, height", r["height"])'

echo "== rotas DEPOIS (deve ter só 1, 56 e 1399811149) =="
terrad q wasm contract-state smart "$WARP" '{"router":{"list_routes":{}}}' --node "$NODE" --output json | python3 -m json.tool | grep -E "domain|route"
echo
echo "⚠️ agora reinicia o relayer: ssh root@31.97.91.4 'systemctl restart hyperlane-relayer'"
