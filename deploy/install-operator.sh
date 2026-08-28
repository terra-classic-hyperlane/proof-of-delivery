#!/usr/bin/env bash
# =============================================================================
# tc-proof-of-delivery · one-shot operator installer
#
# Installs the 3 off-chain operator services on this machine, mirroring the
# production layout:
#   oracle-agent    — gas/price oracle for the 4 governors (loop 4h)
#   claim-agent     — emits receipts & collects commissions (loop 5min)
#   epoch-reporter  — TC->Solana epoch quorum reporter (loop 1h)
#
# Idempotent: re-running updates the CODE but NEVER touches your .env,
# config.json or state.json. Services are installed+enabled but NOT started —
# fill in the env files first, then: systemctl start oracle-agent claim-agent epoch-reporter
#
# Usage:  bash deploy/install-operator.sh
#   ORACLE_DIR=/root/oracle-agent CLAIM_DIR=/root/claim-agent (override via env)
# Docs:   docs/install/INSTALL.md
# =============================================================================
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ORACLE_DIR="${ORACLE_DIR:-/root/oracle-agent}"
CLAIM_DIR="${CLAIM_DIR:-/root/claim-agent}"
NODE_BIN="$(command -v node || true)"

die() { echo "ERROR: $*" >&2; exit 1; }

[ -n "$NODE_BIN" ] || die "Node.js not found — install Node.js >= 20 first"
node -e 'process.exit(parseInt(process.versions.node) >= 20 ? 0 : 1)' \
  || die "Node.js >= 20 required (found $(node -v))"
[ "$(id -u)" = 0 ] || die "run as root (installs systemd units)"

echo "== 1/5 directories + code =="
mkdir -p "$ORACLE_DIR/logs" "$CLAIM_DIR/logs"
cp -r "$REPO/oracle-agent/src" "$ORACLE_DIR/"
cp "$REPO/oracle-agent/package.json" "$REPO/oracle-agent/config.example.json" \
   "$REPO/oracle-agent/README.md" "$ORACLE_DIR/"
cp "$REPO/deploy/claim-agent-receipt.mjs" "$REPO/deploy/deliver-receipts-tc.mjs" \
   "$REPO/deploy/solana-epoch-reporter.mjs" "$CLAIM_DIR/"

echo "== 2/5 npm dependencies =="
( cd "$ORACLE_DIR" && npm install --omit=dev --no-audit --no-fund )
( cd "$CLAIM_DIR"
  [ -f package.json ] || npm init -y >/dev/null
  npm install --no-audit --no-fund \
    @cosmjs/cosmwasm-stargate @cosmjs/proto-signing @cosmjs/stargate \
    @noble/hashes bs58 ethers @solana/web3.js )

echo "== 3/5 config + env templates (existing files are NEVER overwritten) =="
[ -f "$ORACLE_DIR/config.json" ] || {
  cp "$ORACLE_DIR/config.example.json" "$ORACLE_DIR/config.json"
  echo "   -> created $ORACLE_DIR/config.json from example — review governors/RPCs/domains"; }
[ -f "$ORACLE_DIR/.env" ] || cat > "$ORACLE_DIR/.env" <<'EOF'
# oracle-agent signing keys — names must match the *Env fields in config.json
TC_PRIVATE_KEY=
BSC_PRIVATE_KEY=
ETH_PRIVATE_KEY=
SOL_PRIVATE_KEY=
EOF
[ -f "$CLAIM_DIR/.env" ] || cat > "$CLAIM_DIR/.env" <<'EOF'
# claim-agent + epoch-reporter signing keys (tooling wallet — NOT the relayer wallet)
TC_PRIVATE_KEY=
BSC_PRIVATE_KEY=
SOLANA_PRIVATE_KEY=
EOF
[ -f "$CLAIM_DIR/rpc.env" ] || cat > "$CLAIM_DIR/rpc.env" <<'EOF'
TC_RPC=https://rpc.terra-classic.hexxagon.io
TC_LCD=https://lcd.terra-classic.hexxagon.io
BSC_RPC=https://bsc-rpc.publicnode.com
ETH_RPC=https://ethereum-rpc.publicnode.com
SOLANA_RPC=https://api.mainnet-beta.solana.com
EOF

echo "== 4/5 systemd units =="
cat > /etc/systemd/system/oracle-agent.service <<EOF
[Unit]
Description=oracle-agent (proof-of-delivery) — updates the gas oracles on the 4 networks every 4h
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=$ORACLE_DIR
EnvironmentFile=$ORACLE_DIR/.env
ExecStart=$NODE_BIN src/index.js
Restart=always
RestartSec=60
StandardOutput=append:$ORACLE_DIR/logs/agent.log
StandardError=append:$ORACLE_DIR/logs/agent.log

[Install]
WantedBy=multi-user.target
EOF
cat > /etc/systemd/system/claim-agent.service <<EOF
[Unit]
Description=claim-agent (proof-of-delivery) — emits receipts and collects commissions (trigger wallet)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=$CLAIM_DIR
EnvironmentFile=$CLAIM_DIR/.env
EnvironmentFile=$CLAIM_DIR/rpc.env
ExecStart=$NODE_BIN claim-agent-receipt.mjs --loop 300
Restart=always
RestartSec=60
StandardOutput=append:$CLAIM_DIR/logs/agent.log
StandardError=append:$CLAIM_DIR/logs/agent.log

[Install]
WantedBy=multi-user.target
EOF
cat > /etc/systemd/system/epoch-reporter.service <<EOF
[Unit]
Description=epoch-reporter (proof-of-delivery) — TC->Solana epoch quorum
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=$CLAIM_DIR
EnvironmentFile=$CLAIM_DIR/.env
EnvironmentFile=$CLAIM_DIR/rpc.env
ExecStart=$NODE_BIN solana-epoch-reporter.mjs --submit --loop 3600
Restart=always
RestartSec=120
StandardOutput=append:$CLAIM_DIR/logs/reporter.log
StandardError=append:$CLAIM_DIR/logs/reporter.log

[Install]
WantedBy=multi-user.target
EOF

echo "== 5/5 enable =="
systemctl daemon-reload
systemctl enable oracle-agent claim-agent epoch-reporter >/dev/null 2>&1

echo
echo "✅ installed (services enabled, NOT started)."
echo "Next steps:"
echo "  1. Fill in: $ORACLE_DIR/.env · $CLAIM_DIR/.env  (and review $ORACLE_DIR/config.json, $CLAIM_DIR/rpc.env)"
echo "  2. systemctl start oracle-agent claim-agent epoch-reporter"
echo "  3. Verify: tail -f $ORACLE_DIR/logs/agent.log $CLAIM_DIR/logs/agent.log $CLAIM_DIR/logs/reporter.log"
