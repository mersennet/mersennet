#!/usr/bin/env bash
# Mersennet chain-halt watchdog (dependency-free: bash + curl only).
#
# For minimal validator hosts that don't have Node.js. Same behavior as
# chain-watchdog.js: polls the local RPC, alerts on RPC-unreachable or a
# stalled head via webhook (rate-limited), writes a status file, and
# fires a resolve notice on recovery.
#
# Config via environment (see mersennet-watchdog.service):
#   RPC_URL POLL_SECONDS STALL_SECONDS RPC_TIMEOUT_S WEBHOOK_URL
#   WEBHOOK_KIND(slack|discord|generic) ALERT_REPEAT_SECONDS STATUS_FILE LABEL
set -u

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
POLL_SECONDS="${POLL_SECONDS:-15}"
STALL_SECONDS="${STALL_SECONDS:-60}"
RPC_TIMEOUT_S="${RPC_TIMEOUT_S:-8}"
WEBHOOK_URL="${WEBHOOK_URL:-}"
WEBHOOK_KIND="${WEBHOOK_KIND:-generic}"
ALERT_REPEAT_SECONDS="${ALERT_REPEAT_SECONDS:-300}"
STATUS_FILE="${STATUS_FILE:-/var/run/mersennet-watchdog.json}"
LABEL="${LABEL:-mersennet}"

# --- Auto-heal (self-resync) config -----------------------------------------
# When this node's head is stuck but the rest of the network has advanced past
# it, the node is wedged on a dead/minority branch (the validator-3 fork class).
# The proven fix is to wipe local chain state and let the node re-sync the
# canonical chain from peers. We only do this when a reference peer is provably
# AHEAD of our stuck height, so we can never wipe during a genuine network-wide
# halt, and each node decides independently — only the wedged one heals, so
# quorum (3 of 4) is preserved.
AUTO_HEAL="${AUTO_HEAL:-0}"                      # 1 to enable self-resync
HEAL_DRY_RUN="${HEAL_DRY_RUN:-0}"                # 1 = log the action, don't execute
HEAL_STALL_SECONDS="${HEAL_STALL_SECONDS:-180}"  # stall must exceed this before healing
HEAL_PEER_RPCS="${HEAL_PEER_RPCS:-}"             # comma-separated reference RPC URLs
HEAL_PEER_LEAD="${HEAL_PEER_LEAD:-5}"            # peer must be >= this many blocks ahead
HEAL_COOLDOWN_SECONDS="${HEAL_COOLDOWN_SECONDS:-1800}"  # min seconds between heals
HEAL_SERVICE="${HEAL_SERVICE:-mersennet}"        # systemd unit to restart
HEAL_STATE_DIR="${HEAL_STATE_DIR:-/opt/mersennet/data/state}"
HEAL_PEERS_FILE="${HEAL_PEERS_FILE:-/opt/mersennet/data/peers.json}"
last_heal_at=0

last_height=""
last_advance_at=$(date +%s)
alerting=0
last_alert_at=0

log() { echo "[watchdog] $*"; }

rpc_height() {
  local resp hex
  resp=$(curl -sS --max-time "$RPC_TIMEOUT_S" "$RPC_URL" \
    -X POST -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' 2>/dev/null) || return 1
  hex=$(printf '%s' "$resp" | grep -o '"result":"0x[0-9a-f]*"' | grep -o '0x[0-9a-f]*') || return 1
  [ -z "$hex" ] && return 1
  printf '%d' "$hex" 2>/dev/null || return 1
}

send_webhook() {
  [ -z "$WEBHOOK_URL" ] && return 0
  local text="$1" payload
  case "$WEBHOOK_KIND" in
    slack)   payload=$(printf '{"text":"%s"}' "$text");;
    discord) payload=$(printf '{"content":"%s"}' "$text");;
    *)       payload=$(printf '{"text":"%s","label":"%s"}' "$text" "$LABEL");;
  esac
  curl -sS --max-time "$RPC_TIMEOUT_S" "$WEBHOOK_URL" \
    -X POST -H 'content-type: application/json' --data "$payload" >/dev/null 2>&1 || \
    log "webhook delivery failed"
}

write_status() { printf '%s\n' "$1" > "$STATUS_FILE" 2>/dev/null || true; }

# Query a peer RPC's height. Echoes an integer on success, nothing on failure.
peer_height() {
  local url="$1" resp hex
  resp=$(curl -sS --max-time "$RPC_TIMEOUT_S" "$url" \
    -X POST -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' 2>/dev/null) || return 1
  hex=$(printf '%s' "$resp" | grep -o '"result":"0x[0-9a-f]*"' | grep -o '0x[0-9a-f]*') || return 1
  [ -z "$hex" ] && return 1
  printf '%d' "$hex" 2>/dev/null || return 1
}

# Highest height reported by any configured reference peer.
max_peer_height() {
  local best="" ph
  local IFS=','
  for url in $HEAL_PEER_RPCS; do
    url=$(printf '%s' "$url" | tr -d ' ')
    [ -z "$url" ] && continue
    if ph=$(peer_height "$url"); then
      if [ -z "$best" ] || [ "$ph" -gt "$best" ]; then best=$ph; fi
    fi
  done
  [ -n "$best" ] && printf '%d' "$best"
}

# Wipe local chain state and restart so the node re-syncs the canonical chain
# from peers. Only called when a reference peer is provably ahead of our stuck
# head (see maybe_auto_heal). Guarded by a cooldown so a flapping node can't
# loop. Returns 0 if a heal was performed.
do_auto_heal() {
  local stuck_h="$1" peer_h="$2" now
  now=$(date +%s)
  if [ $((now - last_heal_at)) -lt "$HEAL_COOLDOWN_SECONDS" ]; then
    log "auto-heal suppressed (cooldown ${HEAL_COOLDOWN_SECONDS}s not elapsed)"
    return 1
  fi
  last_heal_at=$now
  local msg="[$LABEL] AUTO-HEAL: local head stuck at #$stuck_h while peer is at #$peer_h — wiping state and re-syncing"
  log "$msg"; send_webhook "$msg"
  if [ "$HEAL_DRY_RUN" = "1" ]; then
    log "DRY RUN: would stop $HEAL_SERVICE, rm $HEAL_STATE_DIR + $HEAL_PEERS_FILE, restart"
    return 0
  fi
  systemctl stop "$HEAL_SERVICE" 2>/dev/null || true
  sleep 2
  # Move state aside rather than delete outright, so a failed heal is forensically
  # recoverable; keep only the most recent aside copy to bound disk use.
  rm -rf "${HEAL_STATE_DIR}.wedged.prev" 2>/dev/null || true
  [ -d "$HEAL_STATE_DIR" ] && mv "$HEAL_STATE_DIR" "${HEAL_STATE_DIR}.wedged.prev" 2>/dev/null || true
  rm -f "$HEAL_PEERS_FILE" 2>/dev/null || true
  systemctl start "$HEAL_SERVICE" 2>/dev/null || true
  log "auto-heal: restarted $HEAL_SERVICE; node will re-sync from genesis via peers"
  # Reset local tracking so the stall clock restarts cleanly.
  last_height=""
  last_advance_at=$(date +%s)
  return 0
}

# Decide whether to auto-heal on a detected stall. Safe by construction: only
# heals when AUTO_HEAL=1, the stall exceeds HEAL_STALL_SECONDS, and a reference
# peer is at least HEAL_PEER_LEAD blocks ahead of our stuck head (proving the
# network is alive and we are the one behind).
maybe_auto_heal() {
  local stuck_h="$1"
  [ "$AUTO_HEAL" = "1" ] || return 0
  [ -n "$HEAL_PEER_RPCS" ] || return 0
  local peer_h
  peer_h=$(max_peer_height) || return 0
  [ -z "$peer_h" ] && { log "auto-heal: no peer height available; not healing"; return 0; }
  if [ "$peer_h" -ge $((stuck_h + HEAL_PEER_LEAD)) ]; then
    do_auto_heal "$stuck_h" "$peer_h"
  else
    log "auto-heal: peer head #$peer_h not sufficiently ahead of stuck #$stuck_h (need +${HEAL_PEER_LEAD}); treating as network-wide, not healing"
  fi
}

alert() {
  local reason="$1" now first
  now=$(date +%s)
  first=$([ "$alerting" -eq 0 ] && echo 1 || echo 0)
  alerting=1
  if [ "$first" -eq 0 ] && [ $((now - last_alert_at)) -lt "$ALERT_REPEAT_SECONDS" ]; then return; fi
  last_alert_at=$now
  local msg="[$LABEL] CHAIN UNHEALTHY - $reason (rpc=$RPC_URL)"
  log "$msg"
  send_webhook "$msg"
}

resolve() {
  [ "$alerting" -eq 0 ] && return
  alerting=0
  local msg="[$LABEL] chain recovered - head advancing again at #$1"
  log "$msg"
  send_webhook "$msg"
}

log "starting: rpc=$RPC_URL poll=${POLL_SECONDS}s stall=${STALL_SECONDS}s webhook=$([ -n "$WEBHOOK_URL" ] && echo "$WEBHOOK_KIND" || echo disabled)"
if h=$(rpc_height); then last_height=$h; last_advance_at=$(date +%s); log "initial head #$h"; fi

while true; do
  now=$(date +%s)
  if h=$(rpc_height); then
    if [ -z "$last_height" ] || [ "$h" -gt "$last_height" ]; then
      last_height=$h
      last_advance_at=$now
      write_status "$(printf '{"label":"%s","rpc":"%s","healthy":true,"height":%d,"at":%d}' "$LABEL" "$RPC_URL" "$h" "$now")"
      resolve "$h"
    else
      stalled=$((now - last_advance_at))
      if [ "$stalled" -ge "$STALL_SECONDS" ]; then
        write_status "$(printf '{"label":"%s","rpc":"%s","healthy":false,"reason":"head_stalled","height":%d,"stalledSec":%d,"at":%d}' "$LABEL" "$RPC_URL" "$h" "$stalled" "$now")"
        alert "head stuck at #$h for ${stalled}s (>= ${STALL_SECONDS}s)"
        # Self-heal a wedged (forked) node once the stall is long enough and a
        # peer is provably ahead. No-op unless AUTO_HEAL=1.
        if [ "$stalled" -ge "$HEAL_STALL_SECONDS" ]; then
          maybe_auto_heal "$h"
        fi
      else
        write_status "$(printf '{"label":"%s","rpc":"%s","healthy":true,"height":%d,"stalledSec":%d,"at":%d}' "$LABEL" "$RPC_URL" "$h" "$stalled" "$now")"
      fi
    fi
  else
    write_status "$(printf '{"label":"%s","rpc":"%s","healthy":false,"reason":"rpc_unreachable","at":%d}' "$LABEL" "$RPC_URL" "$now")"
    alert "RPC unreachable"
  fi
  sleep "$POLL_SECONDS"
done
