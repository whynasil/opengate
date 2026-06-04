#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "error: must run as root"
    exit 1
fi

cp target/release/opengate /usr/local/bin/
mkdir -p /etc/opengate
cp config/default.toml /etc/opengate/
cp scripts/opengate.service /etc/systemd/system/

systemctl daemon-reload
systemctl enable opengate

echo "OpenGate installed successfully."
echo "Run 'systemctl start opengate' to start the service."
