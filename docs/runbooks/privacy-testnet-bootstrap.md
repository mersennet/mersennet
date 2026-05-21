# Privacy Testnet Bootstrap Runbook

**Audience:** Operators bringing up the Prime Chain privacy testnet
(chain ID **7920**).
**Status:** Workstream H3 (testnet phase).
**Pre-reqs:** Docker 24+, Docker Compose v2+, Linux/macOS host with
≥ 8 GB RAM and 50 GB disk.

This is the step-by-step procedure for booting the 7-validator,
5-of-7 DKG privacy testnet from a fresh host. If you already have a
running testnet and just want to roll a new validator in, jump to
[§6. Adding a validator](#6-adding-a-validator).

---

## 1. Clone + build

```bash
git clone https://github.com/PrimeNumbersLabs/prime-chain.git
cd prime-chain
cargo build --release --workspace
```

Build artifacts you'll need:

- `target/release/prime-chain` — the node binary.
- `target/release/genesis` — generates validator keys + initial
  configs.
- `target/release/migrate-genesis` — migrates a pre-fork snapshot
  to a privacy envelope.

Smoke check:

```bash
./target/release/prime-chain --help
./target/release/migrate-genesis --help
```

---

## 2. Choose your starting state

Two paths:

### 2a. Migrate from a pre-fork snapshot (recommended)

Export a snapshot from the live transparent-chain (chain ID 7919):

```bash
./target/release/prime-chain --config /etc/prime-chain/config.json \
    --export-snapshot ./prefork-snapshot.bin
```

Then run the privacy bootstrap with that snapshot:

```bash
cd testnet
PRE_FORK_SNAPSHOT=$(realpath ../prefork-snapshot.bin) \
ACTIVATION_HEIGHT=100 \
CHAIN_ID=7920 \
./scripts/bootstrap-privacy-genesis.sh
```

The migration deterministically:

1. Reads every transparent EOA balance.
2. Derives `(rho, psi, owner_pk)` per `docs/security/cryptography-spec.md` §3.7.
3. Mints one shielded note per non-zero account into a fresh
   shielded-state tree.
4. Stamps the activation height (privacy mode flips at that block).
5. Writes the result to `testnet/genesis-output/privacy/snapshot.bin`.

### 2b. Fresh chain (no migration)

If you're not inheriting state:

```bash
cd testnet
ACTIVATION_HEIGHT=100 ./scripts/bootstrap-privacy-genesis.sh
```

Validators boot from an empty state. Privacy mode flips at the
configured activation height; until then transactions are
plain-EVM. This is the recommended path for adversarial /
ZK-circuit testing.

---

## 3. Inspect the generated artifacts

```bash
ls testnet/genesis-output/privacy/
# keys/
#   validator-1.json … validator-7.json
#   faucet-key.json
# snapshot.bin   (only present on the migration path)
```

Each validator key is a 32-byte ECDSA secp256k1 secret. They must
**never** be checked into version control. The bootstrap script
writes them mode-0600.

---

## 4. Bring up the testnet

```bash
cd testnet
docker compose -f docker-compose.privacy.yml up -d --build
```

Expected services after ~60 s:

| Service | Port |
|---|---|
| privacy-rpc-node (read RPC + WS) | 8545, 8546 |
| privacy-faucet | 8081 |
| privacy-prometheus | 9100 |
| privacy-grafana | 3001 |
| privacy-validator-{1..7} | 18545..18551 (RPC), 19090..19096 (P2P) |

Health check:

```bash
curl -s http://localhost:8545/health
# {"status":"ok","privacy_mode_activated":false,"block":<n>,...}

curl -s -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"prime_getChainConfig","params":[]}' \
  http://localhost:8545
# expect chain_id=7920, privacy.activation_height=100
```

Watch for `privacy hard fork activated` in the logs:

```bash
docker logs privacy-validator-1 -f | grep -E "(privacy|DKG|FbaCleared)"
```

---

## 5. Verify privacy mode flips at the activation height

Once block height reaches `ACTIVATION_HEIGHT` (default 100), every
validator should log:

```
WARN  Prime Chain privacy hard fork activated — shielded tx type 0x7E now accepted
INFO  DKG ceremony started for new epoch  epoch=0
```

Validate via RPC:

```bash
curl -s -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"prime_getShieldedRoot","params":[]}' \
  http://localhost:8545
```

Empty tree returns the zero-root. After the first shielded tx the
root advances.

---

## 6. Adding a validator

1. Stop the testnet (`docker compose -f docker-compose.privacy.yml down`).
2. Re-run bootstrap with `N_VALIDATORS=8` (must regenerate genesis
   — the privacy testnet doesn't support hot validator-set changes
   until governance Workstream J lands).
3. Add a new service block to `docker-compose.privacy.yml` mirroring
   `privacy-validator-7`.
4. Bring back up.

For zero-downtime validator rotation (post-mainnet), use
`prime_governance` workflow (see Workstream J runbook — not yet
shipped).

---

## 7. Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `redb state DB open: I/O error` | `/data` not mounted writable | `docker volume inspect privacy-validator-N-data` and re-mount |
| Healthcheck returns `privacy_mode_activated=false` past activation height | Block production stalled before the fork | check `docker logs privacy-validator-1` for HotStuff-2 deadlock; restart validator with `docker compose restart privacy-validator-1` |
| `submit_shielded_order` returns `ProofRejected` | Test wallet using stale shielded root | wallet must re-prove against a root in the recent-roots window (64 blocks) |
| `DKG ceremony stuck` | < threshold validators alive at epoch boundary | bring missing validators back; the coordinator will rerun the ceremony on the next epoch boundary |

---

## 8. Tearing down

```bash
docker compose -f docker-compose.privacy.yml down -v
rm -rf testnet/genesis-output/privacy
```

This wipes all volumes, keys, and snapshots — irreversible.

---

## 9. Where to look next

- **Bug-bash + chaos**: `testnet/scripts/chaos-kill-validator.sh`
- **Synthetic load**: `testnet/scripts/privacy-load.sh`
- **Metrics**: `deploy/monitoring/grafana-privacy-dashboard.json`
  (imported automatically by `privacy-grafana`)
- **Crypto audit prep**: `docs/security/cryptography-spec.md`
- **Privacy invariants**: `docs/security/privacy-invariants.md`
