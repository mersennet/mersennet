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
