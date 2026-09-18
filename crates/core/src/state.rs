use anyhow::{Result, bail};
use revm::db::InMemoryDB;
use revm::primitives::{
    AccountInfo, Address, B256, Bytecode, Bytes, KECCAK_EMPTY, U256, keccak256,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use crate::bridge::{BridgeDomain, BridgeMessage, BridgeQueue, BridgeQueueSnapshot};
use crate::engine::Block;
use crate::mersennet_orders::{
    AccountState, Market, MarketId, MarketStatus, MersennetOrdersState, Order, OrderBook, OrderId,
    Position, Side, TimeInForce,
};

pub(crate) struct MerkleTree;

impl MerkleTree {
    pub(crate) fn compute_root(items: &[(Vec<u8>, Vec<u8>)]) -> B256 {
        if items.is_empty() {
            return B256::ZERO;
        }

        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let mut leaves: Vec<B256> = sorted
            .iter()
            .map(|(k, v)| {
                let mut buf = Vec::with_capacity(k.len() + v.len());
                buf.extend_from_slice(k);
                buf.extend_from_slice(v);
                keccak256(buf)
            })
            .collect();

        while leaves.len() > 1 {
            let mut next = Vec::new();
            let mut i = 0;
            while i < leaves.len() {
                if i + 1 < leaves.len() {
                    let mut buf = [0u8; 64];
                    buf[..32].copy_from_slice(leaves[i].as_slice());
                    buf[32..].copy_from_slice(leaves[i + 1].as_slice());
                    next.push(keccak256(buf));
                    i += 2;
                } else {
                    next.push(leaves[i]);
                    i += 1;
                }
            }
            leaves = next;
        }

        leaves[0]
    }

    pub(crate) fn compute_proof(
        leaves: &[B256],
        target_idx: usize,
    ) -> (Vec<B256>, Vec<bool>, B256) {
        if leaves.len() <= 1 {
            return (
                Vec::new(),
                Vec::new(),
                leaves.first().copied().unwrap_or(B256::ZERO),
            );
        }

        let mut current = leaves.to_vec();
        let mut idx = target_idx;
        let mut siblings = Vec::new();
        let mut path = Vec::new();

        while current.len() > 1 {
            let mut next = Vec::new();
            let mut next_idx = 0;
            let mut pair_start = 0;

            while pair_start < current.len() {
                if pair_start + 1 < current.len() {
                    if pair_start == idx {
                        siblings.push(current[pair_start + 1]);
                        path.push(false);
                        next_idx = next.len();
                    } else if pair_start + 1 == idx {
                        siblings.push(current[pair_start]);
                        path.push(true);
                        next_idx = next.len();
                    }
                    let mut buf = [0u8; 64];
                    buf[..32].copy_from_slice(current[pair_start].as_slice());
                    buf[32..].copy_from_slice(current[pair_start + 1].as_slice());
                    next.push(keccak256(buf));
                    pair_start += 2;
                } else {
                    if pair_start == idx {
                        next_idx = next.len();
                    }
                    next.push(current[pair_start]);
                    pair_start += 1;
                }
            }

            idx = next_idx;
            current = next;
        }

        (siblings, path, current[0])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateProof {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub siblings: Vec<B256>,
    pub path: Vec<bool>,
    pub root: B256,
}

impl StateProof {
    pub fn verify(proof: &StateProof) -> bool {
        if proof.siblings.len() != proof.path.len() {
            return false;
        }
        let mut buf = Vec::with_capacity(proof.key.len() + proof.value.len());
        buf.extend_from_slice(&proof.key);
        buf.extend_from_slice(&proof.value);
        let mut hash = keccak256(buf);

        for (sibling, is_right) in proof.siblings.iter().zip(proof.path.iter()) {
            let mut combined = [0u8; 64];
            if *is_right {
                combined[..32].copy_from_slice(sibling.as_slice());
                combined[32..].copy_from_slice(hash.as_slice());
            } else {
                combined[..32].copy_from_slice(hash.as_slice());
                combined[32..].copy_from_slice(sibling.as_slice());
            }
            hash = keccak256(combined);
        }

        hash == proof.root
    }
}

#[derive(Debug)]
pub struct PersistentState {
    db: sled::Db,
    accounts: sled::Tree,
    storage: sled::Tree,
    mersennet_orders: sled::Tree,
    /// Open validator set: registry entries keyed by node identity plus two
    /// meta keys (active set, epoch). Kept out of the orders snapshot so the
    /// state root is unchanged until the first registration.
    validator_registry: sled::Tree,
    bridge_orders_to_evm: sled::Tree,
    bridge_evm_to_orders: sled::Tree,
    blocks: sled::Tree,
    /// `b` + block hash → height (8 BE); `t` + tx hash → height (8 BE) + index (4 BE).
    history_index: sled::Tree,
    pruning: sled::Tree,
    height_meta: sled::Tree,
    dirty_accounts: Mutex<HashSet<Address>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AccountRecord {
    pub(crate) balance: [u8; 32],
    pub(crate) nonce: u64,
    pub(crate) code_hash: [u8; 32],
    pub(crate) code: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SnapshotRecord {
    pub(crate) height: u64,
    pub(crate) state_root: [u8; 32],
    pub(crate) accounts: Vec<(Vec<u8>, AccountRecord)>,
    pub(crate) storage: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) mersennet_orders: Option<Vec<u8>>,
    pub(crate) bridge_orders_to_evm: Option<Vec<u8>>,
    pub(crate) bridge_evm_to_orders: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MersennetOrdersSnapshot {
    pub(crate) next_order_id: u64,
    pub(crate) initial_margin_bps: u64,
    pub(crate) maintenance_margin_bps: u64,
    pub(crate) insurance_fund: [u8; 32],
    pub(crate) insurance_contribution_rate_bps: u64,
    pub(crate) markets: Vec<MarketRecord>,
    pub(crate) orders: Vec<OrderRecord>,
    pub(crate) accounts: Vec<(Vec<u8>, AccountRecordV2)>,
    pub(crate) books: Vec<(u64, OrderBookRecord)>,
    pub(crate) staking: StakingSnapshot,
    #[serde(default)]
    pub(crate) collateral_assets: Vec<CollateralAssetRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct StakingSnapshot {
    pub(crate) pools: Vec<StakingPoolRecord>,
    pub(crate) delegations: Vec<DelegationRecord>,
    pub(crate) unbondings: Vec<UnbondingRecord>,
    pub(crate) unbonding_period_blocks: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StakingPoolRecord {
    pub(crate) validator: Vec<u8>,
    pub(crate) delegated_total: [u8; 32],
    pub(crate) acc_reward_per_share: [u8; 32],
    pub(crate) commission_bps: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DelegationRecord {
    pub(crate) delegator: Vec<u8>,
    pub(crate) validator: Vec<u8>,
    pub(crate) amount: [u8; 32],
    pub(crate) reward_debt: [u8; 32],
    pub(crate) pending_rewards: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct UnbondingRecord {
    pub(crate) delegator: Vec<u8>,
    pub(crate) validator: Vec<u8>,
    pub(crate) amount: [u8; 32],
    pub(crate) unlock_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MarketRecord {
    pub(crate) id: u64,
    pub(crate) symbol: String,
    pub(crate) tick_size: [u8; 32],
    pub(crate) lot_size: [u8; 32],
    pub(crate) last_price: [u8; 32],
    pub(crate) status: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OrderRecord {
    pub(crate) id: u64,
    pub(crate) owner: Vec<u8>,
    pub(crate) market: u64,
    pub(crate) side: u8,
    pub(crate) price: [u8; 32],
    pub(crate) size: [u8; 32],
    pub(crate) tif: u8,
    pub(crate) post_only: bool,
    pub(crate) expire_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PositionRecord {
    pub(crate) size: i128,
    pub(crate) entry_price: [u8; 32],
    pub(crate) realized_pnl: i128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AccountRecordV2 {
    pub(crate) collateral: [u8; 32],
    pub(crate) open_orders: Vec<u64>,
    pub(crate) positions: Vec<(u64, PositionRecord)>,
    #[serde(default)]
    pub(crate) token_collateral: Vec<(Vec<u8>, [u8; 32])>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CollateralAssetRecord {
    pub(crate) token: Vec<u8>,
    pub(crate) weight_bps: u64,
    pub(crate) value_num: [u8; 32],
    pub(crate) value_den: [u8; 32],
    pub(crate) balances_slot: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OrderBookRecord {
    pub(crate) bids: Vec<(Vec<u8>, Vec<u64>)>,
    pub(crate) asks: Vec<(Vec<u8>, Vec<u64>)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BridgeQueueRecord {
    pub(crate) next_nonce: u64,
    pub(crate) messages: Vec<BridgeMessageRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BridgeMessageRecord {
    pub(crate) nonce: u64,
    pub(crate) from: u8,
    pub(crate) to: u8,
    pub(crate) payload: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SnapshotMeta {
    pub height: u64,
    pub state_root: B256,
}

impl PersistentState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)?;
        let accounts = db.open_tree("accounts")?;
        let storage = db.open_tree("storage")?;
        let mersennet_orders = db.open_tree("mersennet_orders")?;
        let validator_registry = db.open_tree("validator_registry")?;
        let bridge_orders_to_evm = db.open_tree("bridge_orders_to_evm")?;
        let bridge_evm_to_orders = db.open_tree("bridge_evm_to_orders")?;
        let blocks = db.open_tree("blocks")?;
        let history_index = db.open_tree("history_index")?;
        let pruning = db.open_tree("pruning")?;
        let height_meta = db.open_tree("height_meta")?;
        Ok(Self {
            db,
            accounts,
            storage,
            mersennet_orders,
            validator_registry,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
            blocks,
            history_index,
            pruning,
            height_meta,
            dirty_accounts: Mutex::new(HashSet::new()),
        })
    }

    pub fn mark_dirty(&self, address: Address) {
        self.dirty_accounts.lock().unwrap().insert(address);
    }

    pub fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()> {
        for entry in self.accounts.iter() {
            let (key, value) = entry?;
            let address = Address::from_slice(&key);
            let record: AccountRecord = bincode::deserialize(&value)?;
            let balance = U256::from_be_bytes(record.balance);
            let code_hash = B256::from(record.code_hash);
            // KECCAK_EMPTY means the account is an EOA — force empty bytecode
            // even if the stored record carries stray bytes. Older versions
            // persisted revm's analyzed padding (a lone STOP byte) for
            // code-less accounts; reloading that as real code made CacheDB
            // recompute a non-empty code hash and EIP-3607 then rejected the
            // account as a tx sender ("senders with deployed code").
            let code = if record.code.is_empty() || code_hash == KECCAK_EMPTY {
                Bytecode::new()
            } else {
                Bytecode::new_raw(Bytes::from(record.code))
            };
            let info = AccountInfo::new(balance, record.nonce, code_hash, code);
            evm_db.insert_account_info(address, info);
        }

        for entry in self.storage.iter() {
            let (key, value) = entry?;
            if key.len() != 52 {
                continue;
            }
            let address = Address::from_slice(&key[..20]);
            let slot = U256::from_be_slice(&key[20..52]);
            let stored = U256::from_be_slice(value.as_ref());
            evm_db.insert_account_storage(address, slot, stored)?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn commit_from_db(&self, evm_db: &InMemoryDB) -> Result<B256> {
        self.write_evm_state(evm_db)?;
        Ok(self.compute_state_root())
    }

    pub fn commit_state(
        &self,
        evm_db: &InMemoryDB,
        mersennet_orders: &MersennetOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
        height: u64,
    ) -> Result<B256> {
        self.write_evm_state(evm_db)?;
        self.commit_mersennet_orders(mersennet_orders)?;
        self.commit_bridge_queues(bridge_orders_to_evm, bridge_evm_to_orders)?;
        let root = self.compute_state_root();
        self.record_height(height, root)?;
        Ok(root)
    }

    fn write_evm_state(&self, evm_db: &InMemoryDB) -> Result<()> {
        // Always persist a full, deterministic image of `evm_db`.
        //
        // The previous incremental path only wrote accounts explicitly
        // marked dirty. Any balance change a code path forgot to mark
        // (or that a producing vs importing node marked differently) left
        // a STALE value in the store, so two nodes with identical
        // in-memory EVM state committed different persisted state — and
        // thus different state roots. That was the root cause of the
        // persistent per-block "imported state root mismatch" warning.
        // A full rewrite makes the committed state a faithful image of
        // `evm_db` on every node, so re-executing a block yields the same
        // root as producing it. The account set is small on this chain,
        // so the cost is negligible.
        {
            let mut guard = self.dirty_accounts.lock().unwrap();
            guard.clear();
        }

        self.accounts.clear()?;
        self.storage.clear()?;

        for (address, db_account) in &evm_db.accounts {
            if let Some(info) = db_account.info() {
                let record = AccountRecord {
                    balance: info.balance.to_be_bytes(),
                    nonce: info.nonce,
                    code_hash: info.code_hash.into(),
                    // original_bytes(), NOT bytes(): analyzed bytecode is
                    // padded with a trailing STOP, and persisting the padding
                    // turns EOAs into "accounts with code" after a reload
                    // (EIP-3607 then rejects them as tx senders).
                    code: info
                        .code
                        .map(|code| code.original_bytes().to_vec())
                        .unwrap_or_default(),
                };
                let data = bincode::serialize(&record)?;
                self.accounts.insert(address.as_slice(), data)?;
            }

            for (slot, value) in &db_account.storage {
                if value.is_zero() {
                    continue;
                }
                let mut key = [0u8; 52];
                key[..20].copy_from_slice(address.as_slice());
                key[20..52].copy_from_slice(&slot.to_be_bytes::<32>());
                self.storage
                    .insert(key.as_slice(), value.to_be_bytes::<32>().to_vec())?;
            }
        }

        self.db.flush()?;
        Ok(())
    }

    /// Open validator set persistence. Keys: `r:<identity>` → bincode
    /// registration; `m:active_set`, `m:epoch`. Nothing is written while the
    /// registry is empty, so activation does not disturb existing roots.
    fn commit_validator_registry(&self, staking: &crate::staking::StakingState) -> Result<()> {
        self.validator_registry.clear()?;
        if staking.registry.is_empty() && staking.active_set.is_empty() {
            return Ok(());
        }
        let mut ids: Vec<&Address> = staking.registry.keys().collect();
        ids.sort();
        for id in ids {
            let reg = &staking.registry[id];
            let mut key = b"r:".to_vec();
            key.extend_from_slice(id.as_slice());
            self.validator_registry
                .insert(key, bincode::serialize(reg)?)?;
        }
        let active: Vec<Vec<u8>> = staking
            .active_set
            .iter()
            .map(|a| a.as_slice().to_vec())
            .collect();
        self.validator_registry
            .insert(b"m:active_set", bincode::serialize(&active)?)?;
        self.validator_registry
            .insert(b"m:epoch", staking.current_epoch.to_be_bytes().to_vec())?;
        Ok(())
    }

    fn load_validator_registry(&self, staking: &mut crate::staking::StakingState) -> Result<()> {
        staking.registry.clear();
        staking.active_set.clear();
        for item in self.validator_registry.iter() {
            let (k, v) = item?;
            if k.starts_with(b"r:") && k.len() == 22 {
                let reg: crate::staking::ValidatorRegistration = bincode::deserialize(&v)?;
                staking.registry.insert(Address::from_slice(&k[2..]), reg);
            } else if k.as_ref() == b"m:active_set" {
                let active: Vec<Vec<u8>> = bincode::deserialize(&v)?;
                staking.active_set = active.iter().map(|a| Address::from_slice(a)).collect();
            } else if k.as_ref() == b"m:epoch" && v.len() == 8 {
                let mut b = [0u8; 8];
                b.copy_from_slice(&v);
                staking.current_epoch = u64::from_be_bytes(b);
            }
        }
        Ok(())
    }

    pub fn load_mersennet_orders(&self, state: &mut MersennetOrdersState) -> Result<()> {
        let Some(value) = self.mersennet_orders.get("state")? else {
            self.load_validator_registry(&mut state.staking)?;
            return Ok(());
        };
        let snapshot: MersennetOrdersSnapshot = bincode::deserialize(&value)?;
        state.next_order_id = snapshot.next_order_id;
        state.initial_margin_bps = snapshot.initial_margin_bps;
        state.maintenance_margin_bps = snapshot.maintenance_margin_bps;
        state.insurance_fund = U256::from_be_bytes(snapshot.insurance_fund);
        state.insurance_contribution_rate_bps = snapshot.insurance_contribution_rate_bps;

        state.markets.clear();
        for market in snapshot.markets {
            state.markets.insert(
                MarketId(market.id),
                Market {
                    id: MarketId(market.id),
                    symbol: market.symbol,
                    tick_size: U256::from_be_bytes(market.tick_size),
                    lot_size: U256::from_be_bytes(market.lot_size),
                    last_price: U256::from_be_bytes(market.last_price),
                    status: decode_market_status(market.status),
                },
            );
        }

        state.orders.clear();
        for order in snapshot.orders {
            state.orders.insert(
                OrderId(order.id),
                Order {
                    id: OrderId(order.id),
                    owner: Address::from_slice(&order.owner),
                    market: MarketId(order.market),
                    side: decode_side(order.side)?,
                    price: U256::from_be_bytes(order.price),
                    size: U256::from_be_bytes(order.size),
                    tif: decode_tif(order.tif)?,
                    post_only: order.post_only,
                    expire_at: order.expire_at,
                },
            );
        }

        state.accounts.clear();
        for (addr_bytes, record) in snapshot.accounts {
            let address = Address::from_slice(&addr_bytes);
            let mut positions = HashMap::new();
            for (market_id, pos) in record.positions {
                positions.insert(
                    MarketId(market_id),
                    Position {
                        size: pos.size,
                        entry_price: U256::from_be_bytes(pos.entry_price),
                        realized_pnl: pos.realized_pnl,
                    },
                );
            }
            state.accounts.insert(
                address,
                AccountState {
                    collateral: U256::from_be_bytes(record.collateral),
                    open_orders: record.open_orders.into_iter().map(OrderId).collect(),
                    positions,
                    token_collateral: record
                        .token_collateral
                        .into_iter()
                        .map(|(t, b)| (Address::from_slice(&t), U256::from_be_bytes(b)))
                        .collect(),
                },
            );
        }

        state.books.clear();
        for (market_id, record) in snapshot.books {
            let mut book = OrderBook::default();
            for (price_bytes, orders) in record.bids {
                let price = U256::from_be_bytes(bytes_to_u256(&price_bytes)?);
                book.bids
                    .insert(price, orders.into_iter().map(OrderId).collect());
            }
            for (price_bytes, orders) in record.asks {
                let price = U256::from_be_bytes(bytes_to_u256(&price_bytes)?);
                book.asks
                    .insert(price, orders.into_iter().map(OrderId).collect());
            }
            state.books.insert(MarketId(market_id), book);
        }

        state.staking = decode_staking(snapshot.staking);
        // The open-set registry lives in its own tree (see commit_validator_registry).
        self.load_validator_registry(&mut state.staking)?;
        state.bad_debt = match self.mersennet_orders.get("bad_debt")? {
            Some(raw) if raw.len() == 32 => U256::from_be_slice(&raw),
            _ => U256::ZERO,
        };
        state.units_migrated = self.mersennet_orders.get("units_migrated")?.is_some();
        state.price_scales.clear();
        if let Some(raw) = self.mersennet_orders.get("price_scales")? {
            let scales: Vec<(u64, u64)> = bincode::deserialize(&raw)?;
            for (m, sc) in scales {
                state
                    .price_scales
                    .insert(crate::mersennet_orders::MarketId(m), sc);
            }
        }
        state.agents.clear();
        if let Some(raw) = self.mersennet_orders.get("agents")? {
            let agents: Vec<(Vec<u8>, Vec<u8>, u64)> = bincode::deserialize(&raw)?;
            for (a, o, exp) in agents {
                state.agents.insert(
                    Address::from_slice(&a),
                    crate::mersennet_orders::AgentGrant {
                        owner: Address::from_slice(&o),
                        expires_at_block: exp,
                    },
                );
            }
        }

        state.collateral_assets.clear();
        for a in snapshot.collateral_assets {
            state.collateral_assets.insert(
                Address::from_slice(&a.token),
                crate::mersennet_orders::CollateralAsset {
                    weight_bps: a.weight_bps,
                    value_num: U256::from_be_bytes(a.value_num),
                    value_den: U256::from_be_bytes(a.value_den),
                    balances_slot: U256::from_be_bytes(a.balances_slot),
                },
            );
        }

        Ok(())
    }

    pub fn commit_mersennet_orders(&self, state: &MersennetOrdersState) -> Result<()> {
        self.mersennet_orders.clear()?;
        // CRITICAL: markets/orders/accounts/books are HashMaps, whose
        // iteration order is non-deterministic across processes. This
        // snapshot is bincode-serialized and folded into the state root,
        // so unsorted iteration made two nodes with identical logical
        // CLOB state compute DIFFERENT state roots — the root cause of
        // the perpetual `imported block state root mismatch`. Sort every
        // collection by a stable key so the bytes (and root) are
        // deterministic.
        let mut markets: Vec<MarketRecord> = state
            .markets
            .values()
            .map(|m| MarketRecord {
                id: m.id.0,
                symbol: m.symbol.clone(),
                tick_size: m.tick_size.to_be_bytes(),
                lot_size: m.lot_size.to_be_bytes(),
                last_price: m.last_price.to_be_bytes(),
                status: encode_market_status(m.status),
            })
            .collect();
        markets.sort_by_key(|m| m.id);
        let mut orders: Vec<OrderRecord> = state
            .orders
            .values()
            .map(|o| OrderRecord {
                id: o.id.0,
                owner: o.owner.as_slice().to_vec(),
                market: o.market.0,
                side: encode_side(o.side),
                price: o.price.to_be_bytes(),
                size: o.size.to_be_bytes(),
                tif: encode_tif(o.tif),
                post_only: o.post_only,
                expire_at: o.expire_at,
            })
            .collect();
        orders.sort_by_key(|o| o.id);
        let mut accounts: Vec<(Vec<u8>, AccountRecordV2)> = state
            .accounts
            .iter()
            .map(|(addr, acct)| {
                let mut positions: Vec<(u64, PositionRecord)> = acct
                    .positions
                    .iter()
                    .map(|(market_id, pos)| {
                        (
                            market_id.0,
                            PositionRecord {
                                size: pos.size,
                                entry_price: pos.entry_price.to_be_bytes(),
                                realized_pnl: pos.realized_pnl,
                            },
                        )
                    })
                    .collect();
                positions.sort_by_key(|(mid, _)| *mid);
                let mut open_orders: Vec<u64> = acct.open_orders.iter().map(|id| id.0).collect();
                open_orders.sort_unstable();
                let mut token_collateral: Vec<(Vec<u8>, [u8; 32])> = acct
                    .token_collateral
                    .iter()
                    .map(|(token, bal)| (token.as_slice().to_vec(), bal.to_be_bytes()))
                    .collect();
                token_collateral.sort_by(|a, b| a.0.cmp(&b.0));
                (
                    addr.as_slice().to_vec(),
                    AccountRecordV2 {
                        collateral: acct.collateral.to_be_bytes(),
                        open_orders,
                        positions,
                        token_collateral,
                    },
                )
            })
            .collect();
        accounts.sort_by(|a, b| a.0.cmp(&b.0));
        let mut books: Vec<(u64, OrderBookRecord)> = state
            .books
            .iter()
            .map(|(market_id, book)| {
                let bids = book
                    .bids
                    .iter()
                    .map(|(price, orders)| {
                        (
                            price.to_be_bytes::<32>().to_vec(),
                            orders.iter().map(|id| id.0).collect(),
                        )
                    })
                    .collect();
                let asks = book
                    .asks
                    .iter()
                    .map(|(price, orders)| {
                        (
                            price.to_be_bytes::<32>().to_vec(),
                            orders.iter().map(|id| id.0).collect(),
                        )
                    })
                    .collect();
                (market_id.0, OrderBookRecord { bids, asks })
            })
            .collect();
        books.sort_by_key(|(mid, _)| *mid);

        let snapshot = MersennetOrdersSnapshot {
            next_order_id: state.next_order_id,
            initial_margin_bps: state.initial_margin_bps,
            maintenance_margin_bps: state.maintenance_margin_bps,
            insurance_fund: state.insurance_fund.to_be_bytes(),
            insurance_contribution_rate_bps: state.insurance_contribution_rate_bps,
            markets,
            orders,
            accounts,
            books,
            staking: encode_staking(&state.staking),
            collateral_assets: {
                let mut v: Vec<CollateralAssetRecord> = state
                    .collateral_assets
                    .iter()
                    .map(|(token, a)| CollateralAssetRecord {
                        token: token.as_slice().to_vec(),
                        weight_bps: a.weight_bps,
                        value_num: a.value_num.to_be_bytes(),
                        value_den: a.value_den.to_be_bytes(),
                        balances_slot: a.balances_slot.to_be_bytes(),
                    })
                    .collect();
                v.sort_by(|a, b| a.token.cmp(&b.token));
                v
            },
        };

        let data = bincode::serialize(&snapshot)?;
        self.mersennet_orders.insert("state", data)?;
        // Agent grants live in their own key so the snapshot bytes (and the
        // state root) are untouched until the first grant exists.
        // Settlement bookkeeping: absent while zero / false.
        if state.bad_debt.is_zero() {
            self.mersennet_orders.remove("bad_debt")?;
        } else {
            self.mersennet_orders
                .insert("bad_debt", state.bad_debt.to_be_bytes::<32>().to_vec())?;
        }
        if state.units_migrated {
            self.mersennet_orders.insert("units_migrated", vec![1u8])?;
        } else {
            self.mersennet_orders.remove("units_migrated")?;
        }
        // Per-market price scales: same convention (absent while all are 1).
        if state.price_scales.is_empty() {
            self.mersennet_orders.remove("price_scales")?;
        } else {
            let scales: Vec<(u64, u64)> = state
                .price_scales
                .iter()
                .map(|(m, sc)| (m.0, *sc))
                .collect();
            self.mersennet_orders
                .insert("price_scales", bincode::serialize(&scales)?)?;
        }
        if state.agents.is_empty() {
            self.mersennet_orders.remove("agents")?;
        } else {
            let agents: Vec<(Vec<u8>, Vec<u8>, u64)> = state
                .agents
                .iter()
                .map(|(a, g)| {
                    (
                        a.as_slice().to_vec(),
                        g.owner.as_slice().to_vec(),
                        g.expires_at_block,
                    )
                })
                .collect();
            self.mersennet_orders
                .insert("agents", bincode::serialize(&agents)?)?;
        }
        self.db.flush()?;
        self.commit_validator_registry(&state.staking)?;
        Ok(())
    }

    pub fn load_bridge_queues(
        &self,
        orders_to_evm: &mut BridgeQueue,
        evm_to_orders: &mut BridgeQueue,
    ) -> Result<()> {
        if let Some(value) = self.bridge_orders_to_evm.get("queue")? {
            let record: BridgeQueueRecord = bincode::deserialize(&value)?;
            orders_to_evm.restore(decode_bridge_queue(record)?);
        }
        if let Some(value) = self.bridge_evm_to_orders.get("queue")? {
            let record: BridgeQueueRecord = bincode::deserialize(&value)?;
            evm_to_orders.restore(decode_bridge_queue(record)?);
        }
        Ok(())
    }

    pub fn commit_bridge_queues(
        &self,
        orders_to_evm: &BridgeQueue,
        evm_to_orders: &BridgeQueue,
    ) -> Result<()> {
        self.bridge_orders_to_evm.clear()?;
        self.bridge_evm_to_orders.clear()?;

        let orders_snapshot = encode_bridge_queue(orders_to_evm.snapshot());
        let evm_snapshot = encode_bridge_queue(evm_to_orders.snapshot());

        self.bridge_orders_to_evm
            .insert("queue", bincode::serialize(&orders_snapshot)?)?;
        self.bridge_evm_to_orders
            .insert("queue", bincode::serialize(&evm_snapshot)?)?;

        self.db.flush()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn export_snapshot(
        &self,
        path: impl AsRef<Path>,
        height: u64,
        state_root: B256,
    ) -> Result<()> {
        let data = self.export_snapshot_bytes(height, state_root)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn import_snapshot(&self, path: impl AsRef<Path>) -> Result<SnapshotMeta> {
        let data = std::fs::read(path)?;
        self.import_snapshot_bytes(&data)
    }

    #[allow(dead_code)]
    pub fn export_snapshot_bytes(&self, height: u64, state_root: B256) -> Result<Vec<u8>> {
        let mut accounts = Vec::new();
        for entry in self.accounts.iter() {
            let (key, value) = entry?;
            let record: AccountRecord = bincode::deserialize(&value)?;
            accounts.push((key.to_vec(), record));
        }

        let mut storage = Vec::new();
        for entry in self.storage.iter() {
            let (key, value) = entry?;
            storage.push((key.to_vec(), value.to_vec()));
        }

        let snapshot = SnapshotRecord {
            height,
            state_root: state_root.0,
            accounts,
            storage,
            mersennet_orders: self.mersennet_orders.get("state")?.map(|v| v.to_vec()),
            bridge_orders_to_evm: self.bridge_orders_to_evm.get("queue")?.map(|v| v.to_vec()),
            bridge_evm_to_orders: self.bridge_evm_to_orders.get("queue")?.map(|v| v.to_vec()),
        };
        Ok(bincode::serialize(&snapshot)?)
    }

    #[allow(dead_code)]
    pub fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta> {
        let snapshot: SnapshotRecord = bincode::deserialize(data)?;

        self.accounts.clear()?;
        self.storage.clear()?;

        for (key, record) in snapshot.accounts {
            let data = bincode::serialize(&record)?;
            self.accounts.insert(key, data)?;
        }

        for (key, value) in snapshot.storage {
            self.storage.insert(key, value)?;
        }

        if let Some(data) = snapshot.mersennet_orders {
            self.mersennet_orders.insert("state", data)?;
        }
        if let Some(data) = snapshot.bridge_orders_to_evm {
            self.bridge_orders_to_evm.insert("queue", data)?;
        }
        if let Some(data) = snapshot.bridge_evm_to_orders {
            self.bridge_evm_to_orders.insert("queue", data)?;
        }

        self.db.flush()?;
        let computed = self.compute_state_root();
        let expected = B256::from(snapshot.state_root);
        if computed != expected {
            bail!("snapshot state root mismatch");
        }

        Ok(SnapshotMeta {
            height: snapshot.height,
            state_root: expected,
        })
    }

    fn collect_state_items(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut items = Vec::new();
        for tree in [
            &self.accounts,
            &self.storage,
            &self.mersennet_orders,
            &self.validator_registry,
            &self.bridge_orders_to_evm,
            &self.bridge_evm_to_orders,
        ] {
            for (key, value) in tree.iter().flatten() {
                items.push((key.to_vec(), value.to_vec()));
            }
        }
        items
    }

    pub fn compute_state_root(&self) -> B256 {
        let mut items = self.collect_state_items();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        MerkleTree::compute_root(&items)
    }

    pub fn store_block(&self, block: &Block) -> Result<()> {
        let data = serde_json::to_vec(block)?;
        self.blocks.insert(block.number.to_be_bytes(), data)?;
        self.write_history_index(block)?;
        self.db.flush()?;
        Ok(())
    }

    fn write_history_index(&self, block: &Block) -> Result<()> {
        let mut batch = sled::Batch::default();
        batch.insert(
            history_block_key(block.hash).to_vec(),
            block.number.to_be_bytes().to_vec(),
        );
        for (index, tx) in block.transactions.iter().enumerate() {
            batch.insert(
                history_tx_key(tx.canonical_hash()).to_vec(),
                history_tx_value(block.number, index as u32).to_vec(),
            );
        }
        self.history_index.apply_batch(batch)?;
        Ok(())
    }

    pub fn index_block(&self, block: &Block) -> Result<()> {
        self.write_history_index(block)
    }

    pub fn block_number_by_hash(&self, hash: B256) -> Result<Option<u64>> {
        Ok(self
            .history_index
            .get(history_block_key(hash))?
            .and_then(|v| decode_be_u64(&v)))
    }

    pub fn tx_location(&self, hash: B256) -> Result<Option<(u64, u32)>> {
        Ok(self
            .history_index
            .get(history_tx_key(hash))?
            .and_then(|v| decode_tx_location(&v)))
    }

    pub fn history_index_floor(&self) -> Result<Option<u64>> {
        Ok(self
            .height_meta
            .get("history_index_floor")?
            .and_then(|v| decode_be_u64(&v)))
    }

    pub fn set_history_index_floor(&self, height: u64) -> Result<()> {
        self.height_meta
            .insert("history_index_floor", &height.to_be_bytes())?;
        Ok(())
    }

    pub fn load_block(&self, number: u64) -> Result<Option<Block>> {
        match self.blocks.get(number.to_be_bytes())? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    pub fn load_blocks_range(&self, from: u64, to: u64) -> Result<Vec<Block>> {
        let mut blocks = Vec::new();
        for number in from..=to {
            if let Some(data) = self.blocks.get(number.to_be_bytes())? {
                blocks.push(serde_json::from_slice(&data)?);
            }
        }
        Ok(blocks)
    }

    pub fn record_height(&self, height: u64, state_root: B256) -> Result<()> {
        self.pruning
            .insert(height.to_be_bytes(), state_root.as_slice())?;
        self.height_meta
            .insert("latest_height", &height.to_be_bytes())?;
        Ok(())
    }

    pub fn begin_commit(&self, height: u64) -> Result<()> {
        self.height_meta
            .insert("commit_in_progress", &height.to_be_bytes())?;
        Ok(())
    }

    pub fn end_commit(&self) -> Result<()> {
        self.height_meta.remove("commit_in_progress")?;
        // Must be durable: the process may exit right after this (graceful
        // shutdown calls exit() without dropping the DB), and an unflushed
        // removal would look like an interrupted commit on the next start.
        self.db.flush()?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn interrupted_commit(&self) -> Result<Option<u64>> {
        match self.height_meta.get("commit_in_progress")? {
            Some(v) if v.len() == 8 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&v);
                Ok(Some(u64::from_be_bytes(buf)))
            }
            _ => Ok(None),
        }
    }

    /// Node-local cache of the consensus set installed at the last epoch
    /// transition (address, stake), so a restarted node resumes with exactly
    /// the set and stakes every other node is using until the next boundary.
    /// Lives in `height_meta`, which is not part of the state root.
    pub fn save_consensus_set(&self, set: &[(Address, U256)]) -> Result<()> {
        let mut buf = Vec::with_capacity(set.len() * 52);
        for (address, stake) in set {
            buf.extend_from_slice(address.as_slice());
            buf.extend_from_slice(&stake.to_be_bytes::<32>());
        }
        self.height_meta.insert("consensus_set", buf)?;
        Ok(())
    }

    pub fn load_consensus_set(&self) -> Result<Option<Vec<(Address, U256)>>> {
        match self.height_meta.get("consensus_set")? {
            Some(v) if !v.is_empty() && v.len() % 52 == 0 => Ok(Some(
                v.chunks(52)
                    .map(|c| (Address::from_slice(&c[..20]), U256::from_be_slice(&c[20..])))
                    .collect(),
            )),
            _ => Ok(None),
        }
    }

    pub fn persisted_height(&self) -> Result<Option<u64>> {
        match self.height_meta.get("latest_height")? {
            Some(v) if v.len() == 8 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&v);
                Ok(Some(u64::from_be_bytes(buf)))
            }
            _ => Ok(None),
        }
    }

    pub fn prune_before(&self, height: u64) -> Result<u64> {
        let mut pruned = 0u64;

        let mut to_remove = Vec::new();
        for entry in self.pruning.iter() {
            let (key, _) = entry?;
            if key.len() != 8 {
                continue;
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&key);
            let entry_height = u64::from_be_bytes(buf);
            if entry_height < height {
                to_remove.push(key.to_vec());
            }
        }
        for key in &to_remove {
            self.pruning.remove(key.as_slice())?;
            pruned += 1;
        }

        let mut block_keys = Vec::new();
        for entry in self.blocks.iter() {
            let (key, _) = entry?;
            if key.len() != 8 {
                continue;
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&key);
            let block_number = u64::from_be_bytes(buf);
            if block_number < height {
                block_keys.push(key.to_vec());
            }
        }
        for key in &block_keys {
            self.blocks.remove(key.as_slice())?;
            pruned += 1;
        }

        self.db.flush()?;
        Ok(pruned)
    }

    pub fn generate_proof(&self, key: &[u8]) -> Result<StateProof> {
        let mut items = self.collect_state_items();
        if items.is_empty() {
            bail!("empty state");
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));

        let target_idx = items
            .iter()
            .position(|(k, _)| k.as_slice() == key)
            .ok_or_else(|| anyhow::anyhow!("key not found in state"))?;

        let value = items[target_idx].1.clone();

        let leaves: Vec<B256> = items
            .iter()
            .map(|(k, v)| {
                let mut buf = Vec::with_capacity(k.len() + v.len());
                buf.extend_from_slice(k);
                buf.extend_from_slice(v);
                keccak256(buf)
            })
            .collect();

        let (siblings, path, root) = MerkleTree::compute_proof(&leaves, target_idx);

        Ok(StateProof {
            key: key.to_vec(),
            value,
            siblings,
            path,
            root,
        })
    }
}

pub(crate) fn encode_side(side: Side) -> u8 {
    match side {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

pub(crate) fn decode_side(value: u8) -> Result<Side> {
    match value {
        0 => Ok(Side::Buy),
        1 => Ok(Side::Sell),
        _ => bail!("invalid side"),
    }
}

pub(crate) fn encode_tif(tif: TimeInForce) -> u8 {
    match tif {
        TimeInForce::Gtc => 0,
        TimeInForce::Ioc => 1,
        TimeInForce::Fok => 2,
    }
}

pub(crate) fn decode_tif(value: u8) -> Result<TimeInForce> {
    match value {
        0 => Ok(TimeInForce::Gtc),
        1 => Ok(TimeInForce::Ioc),
        2 => Ok(TimeInForce::Fok),
        _ => bail!("invalid tif"),
    }
}

pub(crate) fn bytes_to_u256(bytes: &[u8]) -> Result<[u8; 32]> {
    if bytes.len() != 32 {
        bail!("invalid U256 bytes length");
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

pub(crate) fn encode_market_status(status: MarketStatus) -> u8 {
    match status {
        MarketStatus::Active => 0,
        MarketStatus::Halted => 1,
        MarketStatus::SettleOnly => 2,
    }
}

pub(crate) fn decode_market_status(value: u8) -> MarketStatus {
    match value {
        1 => MarketStatus::Halted,
        2 => MarketStatus::SettleOnly,
        _ => MarketStatus::Active,
    }
}

/// Deterministic staking snapshot: every collection sorted by a stable key
/// so serialized bytes (folded into the state root) match across nodes.
pub(crate) fn encode_staking(state: &crate::staking::StakingState) -> StakingSnapshot {
    let mut pools: Vec<StakingPoolRecord> = state
        .pools
        .iter()
        .map(|(validator, p)| StakingPoolRecord {
            validator: validator.as_slice().to_vec(),
            delegated_total: p.delegated_total.to_be_bytes(),
            acc_reward_per_share: p.acc_reward_per_share.to_be_bytes(),
            commission_bps: p.commission_bps,
        })
        .collect();
    pools.sort_by(|a, b| a.validator.cmp(&b.validator));

    let mut delegations: Vec<DelegationRecord> = state
        .delegations
        .iter()
        .map(|((delegator, validator), d)| DelegationRecord {
            delegator: delegator.as_slice().to_vec(),
            validator: validator.as_slice().to_vec(),
            amount: d.amount.to_be_bytes(),
            reward_debt: d.reward_debt.to_be_bytes(),
            pending_rewards: d.pending_rewards.to_be_bytes(),
        })
        .collect();
    delegations.sort_by(|a, b| (&a.delegator, &a.validator).cmp(&(&b.delegator, &b.validator)));

    let mut unbondings: Vec<UnbondingRecord> = state
        .unbondings
        .iter()
        .flat_map(|(delegator, entries)| {
            entries.iter().map(move |e| UnbondingRecord {
                delegator: delegator.as_slice().to_vec(),
                validator: e.validator.as_slice().to_vec(),
                amount: e.amount.to_be_bytes(),
                unlock_at: e.unlock_at,
            })
        })
        .collect();
    unbondings.sort_by(|a, b| {
        (&a.delegator, a.unlock_at, &a.validator).cmp(&(&b.delegator, b.unlock_at, &b.validator))
    });

    StakingSnapshot {
        pools,
        delegations,
        unbondings,
        unbonding_period_blocks: state.unbonding_period_blocks,
    }
}

pub(crate) fn decode_staking(snapshot: StakingSnapshot) -> crate::staking::StakingState {
    let mut state = crate::staking::StakingState {
        unbonding_period_blocks: snapshot.unbonding_period_blocks,
        ..Default::default()
    };
    for p in snapshot.pools {
        state.pools.insert(
            Address::from_slice(&p.validator),
            crate::staking::ValidatorPool {
                delegated_total: U256::from_be_bytes(p.delegated_total),
                acc_reward_per_share: U256::from_be_bytes(p.acc_reward_per_share),
                commission_bps: p.commission_bps,
            },
        );
    }
    for d in snapshot.delegations {
        state.delegations.insert(
            (
                Address::from_slice(&d.delegator),
                Address::from_slice(&d.validator),
            ),
            crate::staking::Delegation {
                amount: U256::from_be_bytes(d.amount),
                reward_debt: U256::from_be_bytes(d.reward_debt),
                pending_rewards: U256::from_be_bytes(d.pending_rewards),
            },
        );
    }
    for u in snapshot.unbondings {
        state
            .unbondings
            .entry(Address::from_slice(&u.delegator))
            .or_default()
            .push(crate::staking::UnbondingEntry {
                validator: Address::from_slice(&u.validator),
                amount: U256::from_be_bytes(u.amount),
                unlock_at: u.unlock_at,
            });
    }
    state
}

pub(crate) fn encode_bridge_queue(snapshot: BridgeQueueSnapshot) -> BridgeQueueRecord {
    BridgeQueueRecord {
        next_nonce: snapshot.next_nonce,
        messages: snapshot
            .messages
            .into_iter()
            .map(|msg| BridgeMessageRecord {
                nonce: msg.nonce,
                from: encode_domain(msg.from),
                to: encode_domain(msg.to),
                payload: msg.payload.as_ref().to_vec(),
            })
            .collect(),
    }
}

pub(crate) fn decode_bridge_queue(record: BridgeQueueRecord) -> Result<BridgeQueueSnapshot> {
    let messages = record
        .messages
        .into_iter()
        .map(|msg| {
            Ok(BridgeMessage {
                nonce: msg.nonce,
                from: decode_domain(msg.from)?,
                to: decode_domain(msg.to)?,
                payload: Bytes::from(msg.payload),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(BridgeQueueSnapshot {
        next_nonce: record.next_nonce,
        messages,
    })
}

pub(crate) fn encode_domain(domain: BridgeDomain) -> u8 {
    match domain {
        BridgeDomain::MersennetOrders => 0,
        BridgeDomain::MersennetEvm => 1,
    }
}

pub(crate) fn decode_domain(value: u8) -> Result<BridgeDomain> {
    match value {
        0 => Ok(BridgeDomain::MersennetOrders),
        1 => Ok(BridgeDomain::MersennetEvm),
        _ => bail!("invalid bridge domain"),
    }
}

impl crate::state_trait::StateBackend for PersistentState {
    fn mark_dirty(&self, address: Address) {
        self.dirty_accounts.lock().unwrap().insert(address);
    }

    fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()> {
        PersistentState::load_into_db(self, evm_db)
    }

    fn commit_state(
        &self,
        evm_db: &InMemoryDB,
        mersennet_orders: &MersennetOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
        height: u64,
    ) -> Result<B256> {
        PersistentState::commit_state(
            self,
            evm_db,
            mersennet_orders,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
            height,
        )
    }

    fn load_mersennet_orders(&self, state: &mut MersennetOrdersState) -> Result<()> {
        PersistentState::load_mersennet_orders(self, state)
    }

    fn commit_mersennet_orders(&self, state: &MersennetOrdersState) -> Result<()> {
        PersistentState::commit_mersennet_orders(self, state)
    }

    fn load_bridge_queues(
        &self,
        orders_to_evm: &mut BridgeQueue,
        evm_to_orders: &mut BridgeQueue,
    ) -> Result<()> {
        PersistentState::load_bridge_queues(self, orders_to_evm, evm_to_orders)
    }

    fn commit_bridge_queues(
        &self,
        orders_to_evm: &BridgeQueue,
        evm_to_orders: &BridgeQueue,
    ) -> Result<()> {
        PersistentState::commit_bridge_queues(self, orders_to_evm, evm_to_orders)
    }

    fn compute_state_root(&self) -> B256 {
        PersistentState::compute_state_root(self)
    }

    fn store_block(&self, block: &crate::engine::Block) -> Result<()> {
        PersistentState::store_block(self, block)
    }

    fn load_block(&self, number: u64) -> Result<Option<crate::engine::Block>> {
        PersistentState::load_block(self, number)
    }

    fn load_blocks_range(&self, from: u64, to: u64) -> Result<Vec<crate::engine::Block>> {
        PersistentState::load_blocks_range(self, from, to)
    }

    fn index_block(&self, block: &crate::engine::Block) -> Result<()> {
        PersistentState::index_block(self, block)
    }

    fn block_number_by_hash(&self, hash: B256) -> Result<Option<u64>> {
        PersistentState::block_number_by_hash(self, hash)
    }

    fn tx_location(&self, hash: B256) -> Result<Option<(u64, u32)>> {
        PersistentState::tx_location(self, hash)
    }

    fn history_index_floor(&self) -> Result<Option<u64>> {
        PersistentState::history_index_floor(self)
    }

    fn set_history_index_floor(&self, height: u64) -> Result<()> {
        PersistentState::set_history_index_floor(self, height)
    }

    fn record_height(&self, height: u64, state_root: B256) -> Result<()> {
        PersistentState::record_height(self, height, state_root)
    }

    fn persisted_height(&self) -> Result<Option<u64>> {
        PersistentState::persisted_height(self)
    }
    fn save_consensus_set(&self, set: &[(Address, U256)]) -> Result<()> {
        PersistentState::save_consensus_set(self, set)
    }
    fn load_consensus_set(&self) -> Result<Option<Vec<(Address, U256)>>> {
        PersistentState::load_consensus_set(self)
    }
    fn begin_commit(&self, height: u64) -> Result<()> {
        PersistentState::begin_commit(self, height)
    }
    fn end_commit(&self) -> Result<()> {
        PersistentState::end_commit(self)
    }
    fn interrupted_commit(&self) -> Result<Option<u64>> {
        PersistentState::interrupted_commit(self)
    }
    fn flush(&self) -> Result<()> {
        PersistentState::flush(self)
    }

    fn prune_before(&self, height: u64) -> Result<u64> {
        PersistentState::prune_before(self, height)
    }

    fn generate_proof(&self, key: &[u8]) -> Result<StateProof> {
        PersistentState::generate_proof(self, key)
    }

    fn export_snapshot_bytes(&self, height: u64, state_root: B256) -> Result<Vec<u8>> {
        PersistentState::export_snapshot_bytes(self, height, state_root)
    }

    fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta> {
        PersistentState::import_snapshot_bytes(self, data)
    }
}

#[cfg(test)]
mod validator_registry_persistence_tests {
    use super::*;
    use crate::mersennet_orders::MersennetOrdersState;
    use crate::staking::ValidatorSetParams;

    #[test]
    fn registry_round_trips_through_sled_and_is_absent_from_root_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let st = PersistentState::open(dir.path()).expect("open");
        let mut orders = MersennetOrdersState::default();
        st.commit_mersennet_orders(&orders).unwrap();
        let root_empty = st.compute_state_root();
        assert_eq!(
            st.validator_registry.len(),
            0,
            "no registry keys while the set is closed"
        );

        let mut params = ValidatorSetParams::default();
        params.activation_height = 1;
        orders.staking.set_params(params);
        let op = Address::from_slice(&[1u8; 20]);
        let id = Address::from_slice(&[2u8; 20]);
        orders
            .staking
            .register_validator(
                op,
                id,
                U256::from(5_000u64) * U256::from(10u64).pow(U256::from(18u64)),
                700,
                5,
            )
            .unwrap();
        orders.staking.active_set = vec![id];
        orders.staking.current_epoch = 3;
        st.commit_mersennet_orders(&orders).unwrap();
        assert_ne!(
            st.compute_state_root(),
            root_empty,
            "registrations are part of the state root"
        );

        let mut loaded = MersennetOrdersState::default();
        st.load_mersennet_orders(&mut loaded).unwrap();
        assert_eq!(
            loaded.staking.registry.get(&id),
            orders.staking.registry.get(&id)
        );
        assert_eq!(loaded.staking.active_set, vec![id]);
        assert_eq!(loaded.staking.current_epoch, 3);
    }
}

/// History-index key/value encoding shared by the sled and redb backends.
pub(crate) fn history_block_key(hash: B256) -> [u8; 33] {
    let mut k = [0u8; 33];
    k[0] = b'b';
    k[1..].copy_from_slice(hash.as_slice());
    k
}
pub(crate) fn history_tx_key(hash: B256) -> [u8; 33] {
    let mut k = [0u8; 33];
    k[0] = b't';
    k[1..].copy_from_slice(hash.as_slice());
    k
}
pub(crate) fn history_tx_value(number: u64, index: u32) -> [u8; 12] {
    let mut v = [0u8; 12];
    v[..8].copy_from_slice(&number.to_be_bytes());
    v[8..].copy_from_slice(&index.to_be_bytes());
    v
}
pub(crate) fn decode_be_u64(v: &[u8]) -> Option<u64> {
    (v.len() == 8).then(|| u64::from_be_bytes(v.try_into().unwrap()))
}
pub(crate) fn decode_tx_location(v: &[u8]) -> Option<(u64, u32)> {
    if v.len() != 12 {
        return None;
    }
    Some((
        u64::from_be_bytes(v[..8].try_into().unwrap()),
        u32::from_be_bytes(v[8..].try_into().unwrap()),
    ))
}
