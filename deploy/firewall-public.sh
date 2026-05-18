#!/usr/bin/env bash
set -euo pipefail

# Firewall rules for PUBLIC node (RPC, faucet, monitoring).

ufw --force reset
ufw default deny incoming
ufw default allow outgoing

ufw allow 22/tcp   comment "SSH"
ufw allow 80/tcp   comment "HTTP (Caddy)"
ufw allow 443/tcp  comment "HTTPS (Caddy)"
ufw allow 8545/tcp comment "JSON-RPC"
ufw allow 8080/tcp comment "Faucet"
ufw allow 3000/tcp comment "Grafana"
ufw allow 30303/udp comment "P2P gossip"
ufw allow 30303/tcp comment "P2P sync"

ufw --force enable
echo "Firewall configured for public node"
ufw status verbose
