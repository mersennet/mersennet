#!/usr/bin/env bash
set -euo pipefail

# Per-node bootstrap script — runs on each VPS via SSH.
# Installs dependencies, creates user, directories, and systemd services.

NODE_ROLE="${1:-validator}"

echo "==> Setting up Prime Chain node (role: $NODE_ROLE)"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl ufw docker.io docker-compose >/dev/null 2>&1 || true

if ! id -u primechain >/dev/null 2>&1; then
    useradd -r -m -s /bin/bash primechain
fi

mkdir -p /opt/prime-chain/{bin,config,data,keys}
chown -R primechain:primechain /opt/prime-chain

cp /tmp/prime-chain-deploy/prime-chain /opt/prime-chain/bin/
cp /tmp/prime-chain-deploy/faucet     /opt/prime-chain/bin/ 2>/dev/null || true
cp /tmp/prime-chain-deploy/genesis    /opt/prime-chain/bin/ 2>/dev/null || true
chmod +x /opt/prime-chain/bin/*

cp /tmp/prime-chain-deploy/config.json  /opt/prime-chain/config/
cp /tmp/prime-chain-deploy/node_key.json /opt/prime-chain/keys/ 2>/dev/null || true

chown -R primechain:primechain /opt/prime-chain

if [ "$NODE_ROLE" = "validator" ]; then
    cp /tmp/prime-chain-deploy/prime-chain-validator.service /etc/systemd/system/prime-chain.service
elif [ "$NODE_ROLE" = "public" ]; then
    cp /tmp/prime-chain-deploy/prime-chain-rpc.service /etc/systemd/system/prime-chain.service
fi

if [ -f /tmp/prime-chain-deploy/prime-chain-faucet.service ]; then
    cp /tmp/prime-chain-deploy/prime-chain-faucet.service /etc/systemd/system/prime-chain-faucet.service
    cp /tmp/prime-chain-deploy/faucet-key.json /opt/prime-chain/keys/
    chown primechain:primechain /opt/prime-chain/keys/faucet-key.json
fi

cp /tmp/prime-chain-deploy/firewall.sh /tmp/firewall.sh 2>/dev/null || true
if [ -f /tmp/firewall.sh ]; then
    chmod +x /tmp/firewall.sh
    bash /tmp/firewall.sh
fi

systemctl daemon-reload
systemctl enable prime-chain
systemctl restart prime-chain

if [ -f /etc/systemd/system/prime-chain-faucet.service ]; then
    systemctl enable prime-chain-faucet
    systemctl restart prime-chain-faucet
fi

if [ "$NODE_ROLE" = "public" ] && [ -f /tmp/prime-chain-deploy/docker-compose.monitoring.yml ]; then
    mkdir -p /opt/prime-chain/monitoring
    cp /tmp/prime-chain-deploy/docker-compose.monitoring.yml /opt/prime-chain/monitoring/docker-compose.yml
    cp /tmp/prime-chain-deploy/prometheus.yml /opt/prime-chain/monitoring/
    mkdir -p /opt/prime-chain/monitoring/grafana
    cp /tmp/prime-chain-deploy/grafana/*.json /opt/prime-chain/monitoring/grafana/ 2>/dev/null || true
    cd /opt/prime-chain/monitoring
    docker-compose up -d || true
fi

if [ -n "${RPC_DOMAIN:-}" ] && [ "$NODE_ROLE" = "public" ]; then
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https >/dev/null 2>&1 || true
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg 2>/dev/null || true
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null 2>&1 || true
    apt-get update -qq && apt-get install -y -qq caddy >/dev/null 2>&1 || true
    cp /tmp/prime-chain-deploy/Caddyfile /etc/caddy/Caddyfile
    systemctl reload caddy || systemctl restart caddy || true
fi

echo "==> Node setup complete (role: $NODE_ROLE)"
