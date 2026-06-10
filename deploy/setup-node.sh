#!/usr/bin/env bash
set -euo pipefail

# Per-node bootstrap script — runs on each VPS via SSH.
# Installs dependencies, creates user, directories, and systemd services.

NODE_ROLE="${1:-validator}"

echo "==> Setting up Mersennet node (role: $NODE_ROLE)"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl ufw docker.io docker-compose >/dev/null 2>&1 || true

if ! id -u mersennet >/dev/null 2>&1; then
    useradd -r -m -s /bin/bash mersennet
fi

mkdir -p /opt/mersennet/{bin,config,data,keys}
chown -R mersennet:mersennet /opt/mersennet

cp /tmp/mersennet-deploy/mersennet /opt/mersennet/bin/
cp /tmp/mersennet-deploy/faucet     /opt/mersennet/bin/ 2>/dev/null || true
cp /tmp/mersennet-deploy/genesis    /opt/mersennet/bin/ 2>/dev/null || true
chmod +x /opt/mersennet/bin/*

cp /tmp/mersennet-deploy/config.json  /opt/mersennet/config/
cp /tmp/mersennet-deploy/node_key.json /opt/mersennet/keys/ 2>/dev/null || true

chown -R mersennet:mersennet /opt/mersennet

if [ "$NODE_ROLE" = "validator" ]; then
    cp /tmp/mersennet-deploy/mersennet-validator.service /etc/systemd/system/mersennet.service
elif [ "$NODE_ROLE" = "public" ]; then
    cp /tmp/mersennet-deploy/mersennet-rpc.service /etc/systemd/system/mersennet.service
fi

if [ -f /tmp/mersennet-deploy/mersennet-faucet.service ]; then
    cp /tmp/mersennet-deploy/mersennet-faucet.service /etc/systemd/system/mersennet-faucet.service
    cp /tmp/mersennet-deploy/faucet-key.json /opt/mersennet/keys/
    chown mersennet:mersennet /opt/mersennet/keys/faucet-key.json
fi

cp /tmp/mersennet-deploy/firewall.sh /tmp/firewall.sh 2>/dev/null || true
if [ -f /tmp/firewall.sh ]; then
    chmod +x /tmp/firewall.sh
    bash /tmp/firewall.sh
fi

systemctl daemon-reload
systemctl enable mersennet
systemctl restart mersennet

if [ -f /etc/systemd/system/mersennet-faucet.service ]; then
    systemctl enable mersennet-faucet
    systemctl restart mersennet-faucet
fi

if [ "$NODE_ROLE" = "public" ] && [ -f /tmp/mersennet-deploy/docker-compose.monitoring.yml ]; then
    mkdir -p /opt/mersennet/monitoring
    cp /tmp/mersennet-deploy/docker-compose.monitoring.yml /opt/mersennet/monitoring/docker-compose.yml
    cp /tmp/mersennet-deploy/prometheus.yml /opt/mersennet/monitoring/
    mkdir -p /opt/mersennet/monitoring/grafana
    cp /tmp/mersennet-deploy/grafana/*.json /opt/mersennet/monitoring/grafana/ 2>/dev/null || true
    cd /opt/mersennet/monitoring
    docker-compose up -d || true
fi

if [ -n "${RPC_DOMAIN:-}" ] && [ "$NODE_ROLE" = "public" ]; then
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https >/dev/null 2>&1 || true
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg 2>/dev/null || true
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null 2>&1 || true
    apt-get update -qq && apt-get install -y -qq caddy >/dev/null 2>&1 || true
    cp /tmp/mersennet-deploy/Caddyfile /etc/caddy/Caddyfile
    systemctl reload caddy || systemctl restart caddy || true
fi

echo "==> Node setup complete (role: $NODE_ROLE)"
