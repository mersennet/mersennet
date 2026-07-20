#!/usr/bin/env bash
# Coordinated auth reset: hardened binary + signed-only CLOB genesis.
# Irreversible — wipes chain state on all 5 hosts.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/deploy/nodes.conf"

HOSTS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4" "$PUBLIC_NODE")
BIN="$ROOT/target/release/mersennet"
FAUCET_BIN="$ROOT/target/release/faucet"

BOTS='0x6DD9bB44Ddfaba76D8868915d6fe80c3F8a932eC 0x2d268a6cF714A0A58A84c5345f2113f562A9eCb6 0x05ed2c228F7FD5FC5750FBC36E7cc9eD95FDb08e 0x5D6Bc28f0dB7048fE28dfB13cb4dDCB145CcD340 0xD7E327055AB807D15dBA84cF37524C4cf50E85b2 0x2022E622d892221aceF633987f84C446995608F1 0xF27147C700feAE6699Be064dF85F9A51958BFba3 0x02305256df788283dD4881d53a291dFFB9285614 0x063d85c5Aa15a3046d3BD2aDB1e25718D1e203d0 0xD4a93bdC80411dc27F5Cd28c5fAee9d5A368b53e 0xAE19fEaCea693facAEBc3a24572Ff30Af84287df 0x19a8E9379bDd2E4fb73473D746D44dC590233C28 0x6E59D540F1843f51cfc72F18af04b1E1B99A6306 0xD33b3B34Eec8E2D116f948cE9F9C279cE74DF4B8 0x2f5E283d1Dd4BE66c4A692B83Cdb266359D63989 0xE08fD6ff63b370BEaEdF7710dB362B3ae2B5D224 0xB9d3191b8e762466E1ff523163f4517A11CC5a01 0x85B593A711BCc721A33929300f39CCD04e94B644 0x599D48e9028190f0Ce46CA2B64101d42EA07d0b9 0xa038A60c9EF33de711f4604573a7E1431A37D933 0xED8db2Dfc8999F859048Ce83d555281D627df845'

[[ -x "$BIN" ]] || { echo "missing $BIN — run cargo build --release --bin mersennet"; exit 1; }

log() { echo "[$(date -u +%H:%M:%S)] $*"; }

log "1/6 Staging binary"
for ip in "${HOSTS[@]}"; do
  scp -o StrictHostKeyChecking=accept-new -q "$BIN" "root@$ip:/opt/mersennet/bin/mersennet.new"
  log "  $ip"
done
if [[ -x "$FAUCET_BIN" ]]; then
  scp -q "$FAUCET_BIN" "root@$PUBLIC_NODE:/opt/mersennet/bin/faucet.new" || true
fi

log "2/6 Stopping fleet"
for ip in "${HOSTS[@]}"; do
  ssh -o StrictHostKeyChecking=accept-new "root@$ip" 'systemctl stop mersennet; systemctl stop mersennet-faucet 2>/dev/null; true'
done

log "3/6 Wipe state, install binary, patch genesis"
for ip in "${HOSTS[@]}"; do
  ssh "root@$ip" "export BOTS='$BOTS'; bash -s" <<'REMOTE'
set -euo pipefail
if [[ -f /opt/mersennet/bin/mersennet ]]; then
  cp -f /opt/mersennet/bin/mersennet /opt/mersennet/bin/mersennet.prev
fi
mv -f /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet
chmod +x /opt/mersennet/bin/mersennet
if [[ -f /opt/mersennet/bin/faucet.new ]]; then
  cp -f /opt/mersennet/bin/faucet /opt/mersennet/bin/faucet.prev 2>/dev/null || true
  mv -f /opt/mersennet/bin/faucet.new /opt/mersennet/bin/faucet
  chmod +x /opt/mersennet/bin/faucet
fi
rm -rf /opt/mersennet/data/state /opt/mersennet/data/peers.json
mkdir -p /opt/mersennet/data

python3 - <<'PY'
import json, os
cfg_path = "/opt/mersennet/config/config.json"
d = json.load(open(cfg_path))
bots = os.environ["BOTS"].split()
bot_native = "1000000000000000000000000"
maker_c = "1000000000000"
taker_c = "100000000000"
keep = {
    "0xc16b899fe8f67b483509510c1e3e74017a337026",
    "0x9d39d8524cb925577f63cdc24d273a7a738a12fa",
    "0x728b74dfe7dd7a2c669bf37005df2aaaabfe5755",
    "0xe4c6bb2447de9aa0c38a732005ec112e41ff6ba4",
    "0xecef1bb56f77fad9ed34b2fb4300393ace974ee6",
    "0x0000000000000000000000000000000000000001",
}
accounts, seen = [], set()
for a in d.get("genesis", {}).get("accounts", []):
    low = a["address"].lower()
    if low in keep:
        accounts.append(a); seen.add(low)
if "0x0000000000000000000000000000000000000001" not in seen:
    accounts.append({
        "address": "0x0000000000000000000000000000000000000001",
        "balance": "100000000000000000000000000",
        "nonce": 0,
    })
for b in bots:
    low = b.lower()
    if low in seen:
        continue
    accounts.append({"address": b, "balance": bot_native, "nonce": 0})
    seen.add(low)
collateral = [{"owner": bots[0], "amount": maker_c}]
collateral += [{"owner": b, "amount": taker_c} for b in bots[1:]]
d.setdefault("genesis", {})
d["genesis"]["accounts"] = accounts
d["genesis"]["collateral"] = collateral
d["genesis"]["markets"] = [
    {"symbol": "MRSN", "tick_size": "1", "lot_size": "1"},
    {"symbol": "BTC", "tick_size": "10", "lot_size": "1"},
    {"symbol": "ETH", "tick_size": "1", "lot_size": "1"},
    {"symbol": "SOL", "tick_size": "1", "lot_size": "1"},
    {"symbol": "ARB", "tick_size": "1", "lot_size": "1"},
]
d["genesis"]["validators"] = d["genesis"].get("validators") or [
    {"address": "0xc16b899fe8f67b483509510c1e3e74017a337026", "stake": "1000000000000000000000000"},
    {"address": "0x9d39d8524cb925577f63cdc24d273a7a738a12fa", "stake": "1000000000000000000000000"},
    {"address": "0x728b74dfe7dd7a2c669bf37005df2aaaabfe5755", "stake": "1000000000000000000000000"},
    {"address": "0xe4c6bb2447de9aa0c38a732005ec112e41ff6ba4", "stake": "1000000000000000000000000"},
]
d["mersennet_orders"] = {
    "allow_unsigned_orders_rpc": False,
    "initial_margin_bps": 0,
    "maintenance_margin_bps": 0,
}
d["token_economics"] = {
    "max_supply": "618970019642690137449562111",
    "initial_reward_per_block": "2305843009213693951",
    "halving_interval": 33550336,
}
json.dump(d, open(cfg_path, "w"), indent=2)
print("patched accounts=%d collateral=%d unsigned=%s" % (
    len(accounts), len(collateral), d["mersennet_orders"]["allow_unsigned_orders_rpc"]))
PY
REMOTE
  log "  $ip ready"
done

log "4/6 Starting validators then public node"
for ip in "$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4"; do
  ssh "root@$ip" 'systemctl start mersennet'
  sleep 4
  log "  validator $ip up"
done
sleep 6
ssh "root@$PUBLIC_NODE" 'systemctl start mersennet; systemctl start mersennet-faucet 2>/dev/null || true'
log "  public node up"

log "5/6 Verifying chain + unsigned rejection"
sleep 12
echo -n "blockNumber: "
curl -s -X POST "https://rpc.mersennet.com" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
echo
echo -n "unsigned submitOrder (must fail): "
curl -s -X POST "https://rpc.mersennet.com" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"mersennet_orders_submitOrder","params":[{"owner":"0x000000000000000000000000000000000000dEaD","market_id":1,"side":"Buy","price":"0x1","size":"0x1","tif":"Gtc"}]}'
echo
echo -n "order book market 1: "
curl -s -X POST "https://rpc.mersennet.com" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"mersennet_orders_getOrderBook","params":["0x1"]}' | head -c 200
echo

log "6/6 Fleet reset complete"
