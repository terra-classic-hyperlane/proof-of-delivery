#!/usr/bin/env bash
# installs the web panel as a USER service (systemd) on WSL/Linux — comes up on its own.
set -euo pipefail
NODE=$(which node)
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/tcpod-monitor.service <<UNIT
[Unit]
Description=tc-proof-of-delivery web panel (http://localhost:8787)
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
loginctl enable-linger "$USER" 2>/dev/null || echo "  (linger: run 'sudo loginctl enable-linger $USER' if it does not come up on boot)"
echo "✓ panel at http://localhost:8787 · status: systemctl --user status tcpod-monitor"
