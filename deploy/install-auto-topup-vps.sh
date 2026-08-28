#!/usr/bin/env bash
# installs auto-topup as a systemd timer on the VPS (runs every 30 min, only acts when <threshold)
set -euo pipefail
VPS="${VPS:-root@31.97.91.4}"
scp deploy/auto-topup.mjs "$VPS:/root/claim-agent/auto-topup.mjs"
echo "⚠ create /root/claim-agent/topup.env on the VPS with the RESERVE_EVM_KEY (chmod 600) before enabling."
ssh "$VPS" 'cat > /etc/systemd/system/auto-topup.service <<UNIT
[Unit]
Description=auto-topup of the gas trigger wallets (reserve -> trigger when low)
[Service]
Type=oneshot
WorkingDirectory=/root/claim-agent
EnvironmentFile=/root/claim-agent/rpc.env
EnvironmentFile=-/root/claim-agent/topup.env
ExecStart=/usr/local/bin/node /root/claim-agent/auto-topup.mjs --run
StandardOutput=append:/root/claim-agent/logs/topup.log
StandardError=append:/root/claim-agent/logs/topup.log
UNIT
cat > /etc/systemd/system/auto-topup.timer <<UNIT
[Unit]
Description=runs auto-topup every 30 min
[Timer]
OnBootSec=5min
OnUnitActiveSec=30min
[Install]
WantedBy=timers.target
UNIT
systemctl daemon-reload
echo "created. to enable (after adding topup.env): systemctl enable --now auto-topup.timer"'
