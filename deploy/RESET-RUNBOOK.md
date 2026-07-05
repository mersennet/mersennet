# Testnet reset runbook (new consensus + block-hash + tx-hashing)

The correctness overhaul changes the block format (content-committing
hash + parent hash), the transaction hash scheme (RLP/EIP-155), and the
consensus (single-leader BFT). All three are **consensus-breaking**, so
they require a coordinated wipe + re-genesis at block 0 — the new binary
cannot resume the old chain.

This is an **irreversible, live-outage** operation. Do it in a
maintenance window, with the steps below, and only after the CLOB
prerequisite (below) is done — otherwise trade.mersennet.com's order
book breaks.

## Prerequisite (BLOCKER): CLOB orders must flow through consensus

Today the market-maker bot submits orders via the unsigned
`mersennet_orders_*` RPC, which mutates the **local** order state of
whichever node it hits (the public full node). Under single-leader
production, only the elected validator produces a block, and it never
sees those locally-applied orders → the CLOB would be empty/divergent on
the canonical chain.

Fix before reset (all now unblocked by the RLP tx-hashing work):

1. **Seed markets at genesis** so every node has the same markets
   deterministically (add a `markets` section to `GenesisConfig` and
   create them in `Engine::new_*`), OR add them via a one-time governance
   tx at height 1. `addMarket` has no precompile selector, so it cannot
   be an ordinary tx — genesis seeding is the clean path.
2. **Market-maker submits orders as signed precompile txs** to
   `0x0000000000000000000000000000000000000100`
   (`placeOrder(uint64,bool,uint256,uint256,uint8)`,
   `depositCollateral(uint256)`), not the unsigned RPC. These gossip to
   the leader and are included in blocks deterministically. The bot now
   works because MetaMask/ethers-style RLP txs are canonical (verified
   e2e).
3. Set `mersennet_orders.allow_unsigned_orders_rpc = false` in the
   genesis config so orders can only arrive as txs (deterministic).

Verify on a local 3+ node devnet that: the order book is identical on
all nodes, trades appear in blocks, and state roots match — before
touching production.

## Reset steps (per the prior reset procedure)

1. Build + stage the new binary on all 5 hosts:
   ```
   cargo build --release --bin mersennet
   for ip in <public> <v1> <v2> <v3> <v4>; do
     scp target/release/mersennet root@$ip:/opt/mersennet/bin/mersennet.new
   done
   ```
2. Stop the fleet: `systemctl stop mersennet` on all 5.
3. Wipe chain state on all 5 (keep keys + config):
   ```
   rm -rf /opt/mersennet/data/state /opt/mersennet/data/peers.json
   ```
4. Swap in the new binary on all 5:
   ```
   mv /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet
   chmod +x /opt/mersennet/bin/mersennet
   ```
5. Update genesis config (markets + `allow_unsigned_orders_rpc:false`)
   on all 5.
6. Start validators first, then the public node:
   `systemctl start mersennet`.
7. Restart the market-maker (now tx-based) and taker bots.
8. Reset the explorer + trade indexers to block 0:
   - explorer indexer: truncate its DB / set `last_indexed_block = 0`.
   - trade indexer: truncate `blocks/transactions/trades/candles/...`.

## Post-reset verification

- [ ] All 4 validators rotate as leaders (block `miner` cycles).
- [ ] No `state root mismatch` / `parent hash mismatch` in any log.
- [ ] `mersennet_getLatestStateProof` returns a proof; `verifyStateProof`
      = true; explorer shows the proof (labeled "Development proof").
- [ ] Explorer shows a real parent-hash chain (block N parentHash == block
      N-1 hash).
- [ ] MetaMask: add network, send a value transfer, tx confirms and is
      found by its wallet hash.
- [ ] trade.mersennet.com: order book populated, trades fill, leaderboard
      shows real traders only.
- [ ] Watchdog healthy on all 5 (`/var/run/mersennet-watchdog.json`).

## Rollback

There is no rollback to the old chain (it's wiped). If the new chain
fails to finalize, revert `/opt/mersennet/bin/mersennet` to the previous
binary (kept as `mersennet.prev`), restore the old genesis, and re-seed
from a validator snapshot as in prior recoveries.
