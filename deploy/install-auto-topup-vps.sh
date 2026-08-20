#!/usr/bin/env bash
# instala o auto-topup como timer systemd na VPS (roda a cada 30 min, só age em <threshold)
set -euo pipefail
VPS="${VPS:-root@31.97.91.4}"
scp deploy/auto-topup.mjs "$VPS:/root/claim-agent/auto-topup.mjs"
echo "⚠ crie /root/claim-agent/topup.env na VPS com a RESERVE_EVM_KEY (chmod 600) antes de habilitar."
ssh "$VPS" 'cat > /etc/systemd/system/auto-topup.service <<UNIT
[Unit]
Description=auto-topup das carteiras-gatilho de gas (reserva -> gatilho quando baixo)
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
Description=roda o auto-topup a cada 30 min
[Timer]
OnBootSec=5min
OnUnitActiveSec=30min
[Install]
WantedBy=timers.target
UNIT
systemctl daemon-reload
echo "criado. p/ ligar (após por a topup.env): systemctl enable --now auto-topup.timer"'
