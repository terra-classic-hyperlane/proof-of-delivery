#!/usr/bin/env bash
# instala o painel web como serviço de USUÁRIO (systemd) no WSL/Linux — sobe sozinho.
set -euo pipefail
NODE=$(which node)
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/tcpod-monitor.service <<UNIT
[Unit]
Description=painel web tc-proof-of-delivery (http://localhost:8787)
After=network-online.target
[Service]
Type=simple
WorkingDirectory=$(cd "$(dirname "$0")" && pwd)
Environment=PORT=8787
ExecStart=$NODE $(cd "$(dirname "$0")" && pwd)/monitor-web.mjs
Restart=always
RestartSec=10
[Install]
WantedBy=default.target
UNIT
systemctl --user daemon-reload
systemctl --user enable --now tcpod-monitor.service
loginctl enable-linger "$USER" 2>/dev/null || echo "  (linger: rode 'sudo loginctl enable-linger $USER' se não subir no boot)"
echo "✓ painel em http://localhost:8787 · status: systemctl --user status tcpod-monitor"
