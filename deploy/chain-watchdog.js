#!/usr/bin/env node
'use strict';

/**
 * Mersennet chain-halt watchdog.
 *
 * Polls a JSON-RPC endpoint and raises an alert when the chain is
 * unhealthy. It detects three failure modes:
 *
 *   1. RPC unreachable / timing out (node process hung or down).
 *   2. Head not advancing for longer than STALL_SECONDS (consensus
 *      halt or a stuck producer) even though RPC still answers.
 *   3. Recovery — fires a resolve notification once the chain advances
 *      again after an alert, so on-call knows it is back.
 *
 * Alerts are delivered to a webhook (Slack / Discord / generic JSON)
 * and are rate-limited so a sustained outage does not spam the channel.
 * A status file is written every tick for local scraping / dashboards.
 *
 * This is intentionally dependency-free (Node >= 18 built-in fetch) so
 * it can run on any node host under systemd with no npm install.
 *
 * Configuration (environment variables):
 *   RPC_URL          JSON-RPC endpoint to poll   (default http://127.0.0.1:8545)
 *   POLL_SECONDS     seconds between polls        (default 15)
 *   STALL_SECONDS    no-new-block alert threshold (default 60)
 *   RPC_TIMEOUT_MS   per-request timeout          (default 8000)
 *   WEBHOOK_URL      Slack/Discord/generic webhook (optional)
 *   WEBHOOK_KIND     slack | discord | generic    (default generic)
 *   ALERT_REPEAT_SECONDS  min seconds between repeat alerts (default 300)
 *   STATUS_FILE      path to write status JSON    (default /var/run/mersennet-watchdog.json)
 *   LABEL            human label for this chain   (default mersennet)
 */

const RPC_URL = process.env.RPC_URL || 'http://127.0.0.1:8545';
const POLL_SECONDS = intEnv('POLL_SECONDS', 15);
const STALL_SECONDS = intEnv('STALL_SECONDS', 60);
const RPC_TIMEOUT_MS = intEnv('RPC_TIMEOUT_MS', 8000);
const WEBHOOK_URL = process.env.WEBHOOK_URL || '';
const WEBHOOK_KIND = (process.env.WEBHOOK_KIND || 'generic').toLowerCase();
const ALERT_REPEAT_SECONDS = intEnv('ALERT_REPEAT_SECONDS', 300);
const STATUS_FILE = process.env.STATUS_FILE || '/var/run/mersennet-watchdog.json';
const LABEL = process.env.LABEL || 'mersennet';

const fs = require('fs');

function intEnv(name, def) {
  const v = parseInt(process.env[name] || '', 10);
  return Number.isFinite(v) && v > 0 ? v : def;
}

// State
let lastHeight = null; // last observed head
let lastAdvanceAt = Date.now(); // when the head last increased
let alerting = false; // are we currently in an alert state?
let lastAlertAt = 0; // last time we sent an alert (rate limiting)

async function rpcBlockNumber() {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), RPC_TIMEOUT_MS);
  try {
    const res = await fetch(RPC_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_blockNumber', params: [] }),
      signal: ctrl.signal,
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const j = await res.json();
    if (j.error) throw new Error(j.error.message || 'rpc error');
    return parseInt(j.result, 16);
  } finally {
    clearTimeout(t);
  }
}

async function sendWebhook(text) {
  if (!WEBHOOK_URL) return;
  let body;
  if (WEBHOOK_KIND === 'slack') body = { text };
  else if (WEBHOOK_KIND === 'discord') body = { content: text };
  else body = { text, label: LABEL, ts: Date.now() };
  try {
    await fetch(WEBHOOK_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(RPC_TIMEOUT_MS),
    });
  } catch (e) {
    console.error(`[watchdog] webhook delivery failed: ${e.message}`);
  }
}

// Fire an alert, respecting the repeat-rate limit so a sustained outage
// doesn't flood the channel. `key` distinguishes new alerts from repeats.
async function alert(reason) {
  const now = Date.now();
  const firstAlert = !alerting;
  alerting = true;
  if (!firstAlert && now - lastAlertAt < ALERT_REPEAT_SECONDS * 1000) return;
  lastAlertAt = now;
  const msg = `:rotating_light: [${LABEL}] CHAIN UNHEALTHY — ${reason} (rpc=${RPC_URL})`;
  console.error(`[watchdog] ${msg}`);
  await sendWebhook(msg);
}

async function resolve(height) {
  if (!alerting) return;
  alerting = false;
  const msg = `:white_check_mark: [${LABEL}] chain recovered — head advancing again at #${height}`;
  console.log(`[watchdog] ${msg}`);
  await sendWebhook(msg);
}

function writeStatus(obj) {
  try {
    fs.writeFileSync(STATUS_FILE, JSON.stringify({ label: LABEL, rpc: RPC_URL, ...obj }, null, 2));
  } catch (_) {
    // status file is best-effort; ignore write errors
  }
}

async function tick() {
  const now = Date.now();
  let height;
  try {
    height = await rpcBlockNumber();
  } catch (e) {
    writeStatus({ healthy: false, reason: `rpc_unreachable: ${e.message}`, at: now });
    await alert(`RPC unreachable: ${e.message}`);
    return;
  }

  if (lastHeight === null || height > lastHeight) {
    // Progress. Reset the stall clock and clear any alert.
    lastHeight = height;
    lastAdvanceAt = now;
    writeStatus({ healthy: true, height, at: now });
    await resolve(height);
    return;
  }

  // RPC answered but head has not advanced. Check the stall window.
  const stalledSec = Math.floor((now - lastAdvanceAt) / 1000);
  if (stalledSec >= STALL_SECONDS) {
    writeStatus({ healthy: false, reason: 'head_stalled', height, stalledSec, at: now });
    await alert(`head stuck at #${height} for ${stalledSec}s (>= ${STALL_SECONDS}s)`);
  } else {
    writeStatus({ healthy: true, height, stalledSec, at: now });
  }
}

async function main() {
  console.log(
    `[watchdog] starting: rpc=${RPC_URL} poll=${POLL_SECONDS}s stall=${STALL_SECONDS}s ` +
      `webhook=${WEBHOOK_URL ? WEBHOOK_KIND : 'disabled'}`,
  );
  // Prime the initial height so the first real stall clock is accurate.
  try {
    lastHeight = await rpcBlockNumber();
    lastAdvanceAt = Date.now();
    console.log(`[watchdog] initial head #${lastHeight}`);
  } catch (e) {
    console.error(`[watchdog] initial poll failed: ${e.message}`);
  }
  // eslint-disable-next-line no-constant-condition
  while (true) {
    await tick();
    await new Promise((r) => setTimeout(r, POLL_SECONDS * 1000));
  }
}

main().catch((e) => {
  console.error(`[watchdog] fatal: ${e.stack || e}`);
  process.exit(1);
});
