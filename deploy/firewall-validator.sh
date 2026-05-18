#!/usr/bin/env bash
set -euo pipefail

# Firewall rules for VALIDATOR nodes.
# Only allows SSH + P2P gossip. RPC is NOT exposed externally.

ufw --force reset
ufw default deny incoming
ufw default allow outgoing

ufw allow 22/tcp comment "SSH"
ufw allow 30303/udp comment "P2P gossip"
ufw allow 30303/tcp comment "P2P sync"

ufw --force enable
echo "Firewall configured for validator node"
ufw status verbose
