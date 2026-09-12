#!/usr/bin/env bash
set -euo pipefail

# Mersennet testnet node — one-line installer.
#
#   curl -fsSL https://mersennet.com/downloads/install.sh | sudo bash
#   curl -fsSL https://mersennet.com/downloads/install.sh | sudo bash -s -- --data-dir /mnt/blockstorage/mersennet
#
# Downloads the latest release bundle, verifies its checksum, and runs the
# bundle's install.sh (which accepts the same options: --data-dir DIR,
# --rpc-public). Requires 64-bit Linux x86-64, glibc 2.34+ (Ubuntu 22.04+,
# Debian 12+), systemd, curl and tar. Re-running upgrades an existing node.

BASE="${MERSENNET_DOWNLOADS:-https://mersennet.com/downloads}"
TARBALL="mersennet-node-linux-x86_64-latest.tar.gz"

if [[ $EUID -ne 0 ]]; then
    echo "error: run with sudo:  curl -fsSL $BASE/install.sh | sudo bash" >&2
    exit 1
fi
for tool in curl tar sha256sum systemctl; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is required" >&2; exit 1; }
done
arch="$(uname -m)"
[[ "$arch" == "x86_64" ]] || { echo "error: this bundle is for x86_64 (you have $arch); build from source instead" >&2; exit 1; }
glibc="$(ldd --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+$' || echo 0)"
if [[ "$(printf '%s\n' "2.34" "$glibc" | sort -V | head -1)" != "2.34" ]]; then
    echo "error: glibc $glibc is too old (need 2.34+, e.g. Ubuntu 22.04+ / Debian 12+)" >&2
    exit 1
fi

WORK="$(mktemp -d /tmp/mersennet-install.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

echo "==> Downloading $BASE/$TARBALL"
curl -fsSL -o "$TARBALL" "$BASE/$TARBALL"
curl -fsSL -o SHA256SUMS "$BASE/SHA256SUMS"
echo "==> Verifying checksum"
sha256sum -c SHA256SUMS --ignore-missing
tar xzf "$TARBALL"
BUNDLE="$(find . -maxdepth 1 -type d -name 'mersennet-node-*' | head -1)"
[[ -n "$BUNDLE" ]] || { echo "error: bundle directory not found after extraction" >&2; exit 1; }
( cd "$BUNDLE" && sha256sum -c SHA256SUMS >/dev/null ) && echo "    bundle contents verified"

echo "==> Installing"
bash "$BUNDLE/install.sh" "$@"
