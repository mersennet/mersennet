#!/usr/bin/env bash
set -euo pipefail

# Package a node release bundle for operators who do not have source access:
#   networks/testnet/package-release.sh [path/to/mersennet-binary]
#
# Produces dist/mersennet-node-linux-x86_64-<gitsha>.tar.gz containing the
# binary, the canonical testnet config, the systemd unit, install.sh, a
# README and SHA256SUMS, plus dist/SHA256SUMS for the tarball itself.
# Publish the tarball to https://mersennet.com/downloads/.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${1:-$REPO_ROOT/target/release/mersennet}"
[[ -x "$BIN" ]] || { echo "error: binary not found at $BIN (cargo build --release --bin mersennet)" >&2; exit 1; }

SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
ARCH="linux-x86_64"
NAME="mersennet-node-${ARCH}-${SHA}"
DIST="$REPO_ROOT/dist"
STAGE="$DIST/$NAME"
rm -rf "$STAGE" && mkdir -p "$STAGE"

install -m 0755 "$BIN" "$STAGE/mersennet"
install -m 0644 "$REPO_ROOT/networks/testnet/config.json" "$STAGE/config.json"
install -m 0644 "$REPO_ROOT/networks/testnet/mersennet.service" "$STAGE/mersennet.service"
install -m 0755 "$REPO_ROOT/networks/testnet/install.sh" "$STAGE/install.sh"

BIN_SHA="$(sha256sum "$STAGE/mersennet" | cut -d' ' -f1)"
GLIBC="$(objdump -T "$STAGE/mersennet" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -t. -k2,2n -k3,3n -u | tail -1 || echo unknown)"

cat > "$STAGE/README.md" <<EOF
# Mersennet testnet node — release bundle ${SHA}

Chain ID 131071 · built from mersennet/mersennet@${SHA} · ${ARCH} · requires ${GLIBC/GLIBC_/glibc >= }

| File | Purpose |
|------|---------|
| \`mersennet\` | Node binary. sha256 \`${BIN_SHA}\` — identical to the build running on the testnet validators. |
| \`config.json\` | Canonical testnet config. Never edit \`genesis\`, \`engine.chain_id\` or \`token_economics\`; adjust only \`rpc\`, \`ws\`, \`p2p.listen\` and paths. |
| \`mersennet.service\` | Hardened systemd unit. |
| \`install.sh\` | One-command install: binary to /usr/local/bin, config to /etc/mersennet, system user, data dir, service. |

## Install as a service (Ubuntu 22.04+/Debian 12+)

    sudo bash install.sh
    journalctl -u mersennet -f

## Or run in the foreground (no service)

    sudo install -m 0755 mersennet /usr/local/bin/mersennet
    mkdir -p ~/mersennet-node && cp config.json ~/mersennet-node/ && cd ~/mersennet-node
    mersennet --config config.json --mode full --rpc

The node generates its P2P identity at \`keys/node_key.json\` on first start,
replays history from the bootnodes (46.225.30.187, 46.225.183.192,
49.13.54.79 — TCP+UDP 30303) and then follows live gossip. Outbound-only
connectivity is enough; opening 30303 lets others sync from you.

Full guide: https://docs.mersennet.com/validators/run-a-node/
EOF

( cd "$STAGE" && sha256sum mersennet config.json mersennet.service install.sh README.md > SHA256SUMS )
# Reproducible tarball: fixed mtimes (the commit time), sorted entries, no
# owner info, no gzip timestamp — re-running on the same commit yields the
# same bytes and the same checksum.
COMMIT_TIME="$(git -C "$REPO_ROOT" log -1 --format=%ct)"
( cd "$DIST" && tar --sort=name --mtime="@$COMMIT_TIME" --owner=0 --group=0 --numeric-owner -cf - "$NAME" | gzip -n -9 > "$NAME.tar.gz" && sha256sum "$NAME.tar.gz" > SHA256SUMS )
# The published directory also exposes a -latest symlink to this tarball;
# list it under that name too so `sha256sum -c SHA256SUMS --ignore-missing`
# works whichever filename the operator downloaded.
( cd "$DIST" && sed "s/$NAME.tar.gz/mersennet-node-${ARCH}-latest.tar.gz/" SHA256SUMS >> SHA256SUMS )
rm -rf "$STAGE"

echo "bundle: $DIST/$NAME.tar.gz"
echo "binary sha256: $BIN_SHA"
cat "$DIST/SHA256SUMS"
