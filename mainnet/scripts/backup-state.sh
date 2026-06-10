#!/bin/bash
# Backup Mersennet state data
# Usage: ./backup-state.sh --data-dir /var/lib/prime-chain --backup-dir /backups

set -e

DATA_DIR=""
BACKUP_DIR=""
COMPRESS=true

usage() {
    echo "Usage: $0 --data-dir DIR --backup-dir DIR [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --data-dir DIR      Source data directory (default: /var/lib/prime-chain)"
    echo "  --backup-dir DIR    Destination backup directory"
    echo "  --no-compress       Skip gzip compression"
    echo "  -h, --help          Show this help"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        --backup-dir)
            BACKUP_DIR="$2"
            shift 2
            ;;
        --no-compress)
            COMPRESS=false
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

DATA_DIR="${DATA_DIR:-/var/lib/prime-chain}"
BACKUP_DIR="${BACKUP_DIR:-/backups/prime-chain}"

if [[ ! -d "$DATA_DIR" ]]; then
    echo "Error: Data directory does not exist: $DATA_DIR"
    exit 1
fi

mkdir -p "$BACKUP_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="prime-chain-state-$TIMESTAMP"

echo "==> Backing up Mersennet state"
echo "  Source:      $DATA_DIR"
echo "  Destination: $BACKUP_DIR/$BACKUP_NAME"
echo ""

# Create backup (exclude keys for security - backup separately if needed)
if [[ "$COMPRESS" == true ]]; then
    tar -czf "$BACKUP_DIR/${BACKUP_NAME}.tar.gz" -C "$DATA_DIR" state 2>/dev/null || \
    tar -czf "$BACKUP_DIR/${BACKUP_NAME}.tar.gz" -C "$(dirname "$DATA_DIR")" "$(basename "$DATA_DIR")/state" 2>/dev/null || {
        echo "Error: Failed to create backup. Ensure state directory exists."
        exit 1
    }
    echo "  Created: $BACKUP_DIR/${BACKUP_NAME}.tar.gz"
else
    mkdir -p "$BACKUP_DIR/$BACKUP_NAME"
    cp -a "$DATA_DIR/state" "$BACKUP_DIR/$BACKUP_NAME/" 2>/dev/null || {
        echo "Error: Failed to copy state. Ensure state directory exists."
        exit 1
    }
    echo "  Created: $BACKUP_DIR/$BACKUP_NAME/"
fi

echo ""
echo "==> Backup complete"
