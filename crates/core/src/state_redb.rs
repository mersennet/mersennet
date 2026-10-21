use anyhow::{Result, bail};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use revm::db::InMemoryDB;
use revm::primitives::{
    AccountInfo, Address, B256, Bytecode, Bytes, KECCAK_EMPTY, U256, keccak256,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use crate::bridge::BridgeQueue;
use crate::engine::Block;
use crate::mersennet_orders::{
    AccountState, Market, MarketId, MersennetOrdersState, Order, OrderBook, OrderId, Position,
};
use crate::state::{
    AccountRecord, AccountRecordV2, BridgeQueueRecord, CollateralAssetRecord, MarketRecord,
    MerkleTree, MersennetOrdersSnapshot, OrderBookRecord, OrderRecord, PositionRecord,
    SnapshotMeta, SnapshotRecord, StateProof, bytes_to_u256, decode_be_u64, decode_bridge_queue,
    decode_market_status, decode_side, decode_staking, decode_tif, decode_tx_location,
    encode_bridge_queue, encode_market_status, encode_side, encode_staking, encode_tif,
    history_block_key, history_tx_key, history_tx_value,
};
use crate::state_trait::StateBackend;

const ACCOUNTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("accounts");
const STORAGE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("storage");
const MERSENNET_ORDERS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("mersennet_orders");
const BRIDGE_OTE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bridge_orders_to_evm");
const BRIDGE_ETO: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bridge_evm_to_orders");
const BLOCKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blocks");
/// `b` + block hash → height (8 BE); `t` + tx hash → height (8 BE) + index (4 BE).
const HISTORY_INDEX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("history_index");
const PRUNING: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pruning");
const HEIGHT_META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("height_meta");

/// On-disk shape of the open validator set: (registrations by identity, active set, epoch).
type PersistedValidatorRegistry = (
    Vec<(Vec<u8>, crate::staking::ValidatorRegistration)>,
    Vec<Vec<u8>>,
    u64,
);

pub struct RedbState {
    db: Database,
    dirty_accounts: Mutex<HashSet<Address>>,
}

impl std::fmt::Debug for RedbState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedbState")
            .field("dirty_accounts", &self.dirty_accounts)
            .finish()
    }
}

impl RedbState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // sled creates its directory; redb does not. A fresh node configured
        // with `engine.storage_backend = "redb"` used to panic here with
        // "No such file or directory" (release f67d812).
        std::fs::create_dir_all(path.as_ref())?;
        let db_path = path.as_ref().join("mersennet.redb");
        let db = Database::create(&db_path)?;

        {
            let write_txn = db.begin_write()?;
            let _ = write_txn.open_table(ACCOUNTS)?;
            let _ = write_txn.open_table(STORAGE)?;
            let _ = write_txn.open_table(MERSENNET_ORDERS)?;
            let _ = write_txn.open_table(BRIDGE_OTE)?;
            let _ = write_txn.open_table(BRIDGE_ETO)?;
            let _ = write_txn.open_table(BLOCKS)?;
            let _ = write_txn.open_table(HISTORY_INDEX)?;
            let _ = write_txn.open_table(PRUNING)?;
            let _ = write_txn.open_table(HEIGHT_META)?;
            write_txn.commit()?;
        }

        Ok(Self {
            db,
            dirty_accounts: Mutex::new(HashSet::new()),
        })
    }

    fn write_evm_state(&self, evm_db: &InMemoryDB) -> Result<()> {
        // Always persist a full, deterministic image of `evm_db`. The old
        // incremental (dirty-set) path could leave stale values when a
        // balance change wasn't marked dirty, causing producer/importer
        // state roots to diverge. See the sled backend for the full
        // rationale.
        {
            let mut guard = self.dirty_accounts.lock().unwrap();
            guard.clear();
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
            let keys: Vec<Vec<u8>> = {
                let iter = accounts_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                accounts_table.remove(key.as_slice())?;
            }
        }
        {
            let mut storage_table = write_txn.open_table(STORAGE)?;
            let keys: Vec<Vec<u8>> = {
                let iter = storage_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                storage_table.remove(key.as_slice())?;
            }
        }
        {
            let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
            let mut storage_table = write_txn.open_table(STORAGE)?;

            for (address, db_account) in &evm_db.accounts {
                if let Some(info) = db_account.info() {
                    let record = AccountRecord {
                        balance: info.balance.to_be_bytes(),
                        nonce: info.nonce,
                        code_hash: info.code_hash.into(),
                        // original_bytes(): never persist analyzed padding
                        // (see sled backend note on EIP-3607).
                        code: info
                            .code
                            .map(|code| code.original_bytes().to_vec())
                            .unwrap_or_default(),
                    };
                    let data = bincode::serialize(&record)?;
                    accounts_table.insert(address.as_slice(), data.as_slice())?;
                }

                for (slot, value) in &db_account.storage {
                    if value.is_zero() {
                        continue;
                    }
                    let mut key = [0u8; 52];
                    key[..20].copy_from_slice(address.as_slice());
                    key[20..52].copy_from_slice(&slot.to_be_bytes::<32>());
                    storage_table.insert(key.as_slice(), value.to_be_bytes::<32>().as_slice())?;
                }
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    fn collect_state_items(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut items = Vec::new();
        let Ok(read_txn) = self.db.begin_read() else {
            return items;
        };

        for table_def in [ACCOUNTS, STORAGE, MERSENNET_ORDERS, BRIDGE_OTE, BRIDGE_ETO] {
            if let Ok(table) = read_txn.open_table(table_def)
                && let Ok(iter) = table.iter()
            {
                for (key, value) in iter.flatten() {
                    items.push((key.value().to_vec(), value.value().to_vec()));
                }
            }
        }
        items
    }
}

impl RedbState {
    fn write_history_index(index: &mut redb::Table<'_, &[u8], &[u8]>, block: &Block) -> Result<()> {
        index.insert(
            history_block_key(block.hash).as_slice(),
            block.number.to_be_bytes().as_slice(),
        )?;
        for (i, tx) in block.transactions.iter().enumerate() {
            index.insert(
                history_tx_key(tx.canonical_hash()).as_slice(),
                history_tx_value(block.number, i as u32).as_slice(),
            )?;
        }
        Ok(())
    }
}

impl StateBackend for RedbState {
    fn mark_dirty(&self, address: Address) {
        self.dirty_accounts.lock().unwrap().insert(address);
    }

    fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()> {
        let read_txn = self.db.begin_read()?;

        let accounts_table = read_txn.open_table(ACCOUNTS)?;
        for entry in accounts_table.iter()? {
            let (key, value) = entry?;
            let key_bytes = key.value();
            let value_bytes = value.value();
            let address = Address::from_slice(key_bytes);
            let record: AccountRecord = bincode::deserialize(value_bytes)?;
            let balance = U256::from_be_bytes(record.balance);
            let code_hash = B256::from(record.code_hash);
            // KECCAK_EMPTY = EOA: force empty bytecode even if the record
            // carries stray analyzed-padding bytes (see sled backend note).
            let code = if record.code.is_empty() || code_hash == KECCAK_EMPTY {
                Bytecode::new()
            } else {
                Bytecode::new_raw(Bytes::from(record.code))
            };
            let info = AccountInfo::new(balance, record.nonce, code_hash, code);
            evm_db.insert_account_info(address, info);
        }

        let storage_table = read_txn.open_table(STORAGE)?;
        for entry in storage_table.iter()? {
            let (key, value) = entry?;
            let key_bytes = key.value();
            let value_bytes = value.value();
            if key_bytes.len() != 52 {
                continue;
            }
            let address = Address::from_slice(&key_bytes[..20]);
            let slot = U256::from_be_slice(&key_bytes[20..52]);
            let stored = U256::from_be_slice(value_bytes);
            evm_db.insert_account_storage(address, slot, stored)?;
        }

        Ok(())
    }

    fn commit_state(
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

    fn load_mersennet_orders(&self, state: &mut MersennetOrdersState) -> Result<()> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MERSENNET_ORDERS)?;
        let Some(value) = table.get(b"state".as_slice())? else {
            return Ok(());
        };
        let snapshot: MersennetOrdersSnapshot = bincode::deserialize(value.value())?;
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
        state.bad_debt = match table.get(b"bad_debt".as_slice())? {
            Some(raw) if raw.value().len() == 32 => U256::from_be_slice(raw.value()),
            _ => U256::ZERO,
        };
        state.units_migrated = table.get(b"units_migrated".as_slice())?.is_some();
        state.price_scales.clear();
        if let Some(raw) = table.get(b"price_scales".as_slice())? {
            let scales: Vec<(u64, u64)> = bincode::deserialize(raw.value())?;
            for (m, sc) in scales {
                state
                    .price_scales
                    .insert(crate::mersennet_orders::MarketId(m), sc);
            }
        }
        state.agents.clear();
        if let Some(raw) = table.get(b"agents".as_slice())? {
            let agents: Vec<(Vec<u8>, Vec<u8>, u64)> = bincode::deserialize(raw.value())?;
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
        // Open validator set registry (same content sled keeps in its own tree).
        if let Ok(meta) = read_txn.open_table(HEIGHT_META)
            && let Some(v) = meta.get(b"validator_registry".as_slice())?
        {
            let (regs, active, epoch): PersistedValidatorRegistry =
                bincode::deserialize(v.value())?;
            state.staking.registry.clear();
            for (id, reg) in regs {
                state.staking.registry.insert(Address::from_slice(&id), reg);
            }
            state.staking.active_set = active.iter().map(|a| Address::from_slice(a)).collect();
            state.staking.current_epoch = epoch;
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

    fn commit_mersennet_orders(&self, state: &MersennetOrdersState) -> Result<()> {
        // Deterministic serialization: sort every HashMap-sourced
        // collection by a stable key so the snapshot bytes (folded into
        // the state root) are identical across nodes. See the sled
        // backend's commit_mersennet_orders for the rationale.
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
        // Open validator set registry, written in the same transaction.
        let registry = {
            let mut ids: Vec<&Address> = state.staking.registry.keys().collect();
            ids.sort();
            let regs: Vec<(Vec<u8>, crate::staking::ValidatorRegistration)> = ids
                .iter()
                .map(|id| (id.as_slice().to_vec(), state.staking.registry[*id].clone()))
                .collect();
            let active: Vec<Vec<u8>> = state
                .staking
                .active_set
                .iter()
                .map(|a| a.as_slice().to_vec())
                .collect();
            bincode::serialize(&(regs, active, state.staking.current_epoch))?
        };
        let agents_blob: Option<Vec<u8>> = if state.agents.is_empty() {
            None
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
            Some(bincode::serialize(&agents)?)
        };
        let scales_blob: Option<Vec<u8>> = if state.price_scales.is_empty() {
            None
        } else {
            let scales: Vec<(u64, u64)> = state
                .price_scales
                .iter()
                .map(|(m, sc)| (m.0, *sc))
                .collect();
            Some(bincode::serialize(&scales)?)
        };
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MERSENNET_ORDERS)?;
            table.remove(b"state".as_slice())?;
            table.insert(b"state".as_slice(), data.as_slice())?;
            // Same keys as sled: absent while there are no grants / all scales are 1.
            table.remove(b"agents".as_slice())?;
            if let Some(blob) = &agents_blob {
                table.insert(b"agents".as_slice(), blob.as_slice())?;
            }
            table.remove(b"price_scales".as_slice())?;
            if let Some(blob) = &scales_blob {
                table.insert(b"price_scales".as_slice(), blob.as_slice())?;
            }
            table.remove(b"bad_debt".as_slice())?;
            if !state.bad_debt.is_zero() {
                table.insert(
                    b"bad_debt".as_slice(),
                    state.bad_debt.to_be_bytes::<32>().as_slice(),
                )?;
            }
            table.remove(b"units_migrated".as_slice())?;
            if state.units_migrated {
                table.insert(b"units_migrated".as_slice(), [1u8].as_slice())?;
            }
            let mut meta = write_txn.open_table(HEIGHT_META)?;
            if state.staking.registry.is_empty() && state.staking.active_set.is_empty() {
                meta.remove(b"validator_registry".as_slice())?;
            } else {
                meta.insert(b"validator_registry".as_slice(), registry.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn load_bridge_queues(
        &self,
        orders_to_evm: &mut BridgeQueue,
        evm_to_orders: &mut BridgeQueue,
    ) -> Result<()> {
        let read_txn = self.db.begin_read()?;

        let ote_table = read_txn.open_table(BRIDGE_OTE)?;
        if let Some(value) = ote_table.get(b"queue".as_slice())? {
            let record: BridgeQueueRecord = bincode::deserialize(value.value())?;
            orders_to_evm.restore(decode_bridge_queue(record)?);
        }

        let eto_table = read_txn.open_table(BRIDGE_ETO)?;
        if let Some(value) = eto_table.get(b"queue".as_slice())? {
            let record: BridgeQueueRecord = bincode::deserialize(value.value())?;
            evm_to_orders.restore(decode_bridge_queue(record)?);
        }
        Ok(())
    }

    fn commit_bridge_queues(
        &self,
        orders_to_evm: &BridgeQueue,
        evm_to_orders: &BridgeQueue,
    ) -> Result<()> {
        let orders_snapshot = encode_bridge_queue(orders_to_evm.snapshot());
        let evm_snapshot = encode_bridge_queue(evm_to_orders.snapshot());

        let write_txn = self.db.begin_write()?;
        {
            let mut ote_table = write_txn.open_table(BRIDGE_OTE)?;
            let keys: Vec<Vec<u8>> = {
                let iter = ote_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                ote_table.remove(key.as_slice())?;
            }
            ote_table.insert(
                b"queue".as_slice(),
                bincode::serialize(&orders_snapshot)?.as_slice(),
            )?;
        }
        {
            let mut eto_table = write_txn.open_table(BRIDGE_ETO)?;
            let keys: Vec<Vec<u8>> = {
                let iter = eto_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                eto_table.remove(key.as_slice())?;
            }
            eto_table.insert(
                b"queue".as_slice(),
                bincode::serialize(&evm_snapshot)?.as_slice(),
            )?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn compute_state_root(&self) -> B256 {
        let mut items = self.collect_state_items();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        MerkleTree::compute_root(&items)
    }

    fn store_block(&self, block: &Block) -> Result<()> {
        let data = serde_json::to_vec(block)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BLOCKS)?;
            table.insert(block.number.to_be_bytes().as_slice(), data.as_slice())?;
            let mut index = write_txn.open_table(HISTORY_INDEX)?;
            Self::write_history_index(&mut index, block)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn index_block(&self, block: &Block) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut index = write_txn.open_table(HISTORY_INDEX)?;
            Self::write_history_index(&mut index, block)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn block_number_by_hash(&self, hash: B256) -> Result<Option<u64>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(HISTORY_INDEX)?;
        Ok(table
            .get(history_block_key(hash).as_slice())?
            .and_then(|v| decode_be_u64(v.value())))
    }

    fn tx_location(&self, hash: B256) -> Result<Option<(u64, u32)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(HISTORY_INDEX)?;
        Ok(table
            .get(history_tx_key(hash).as_slice())?
            .and_then(|v| decode_tx_location(v.value())))
    }

    fn history_index_floor(&self) -> Result<Option<u64>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(HEIGHT_META)?;
        Ok(table
            .get(b"history_index_floor".as_slice())?
            .and_then(|v| decode_be_u64(v.value())))
    }

    fn set_history_index_floor(&self, height: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(HEIGHT_META)?;
            table.insert(
                b"history_index_floor".as_slice(),
                height.to_be_bytes().as_slice(),
            )?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn load_block(&self, number: u64) -> Result<Option<Block>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BLOCKS)?;
        match table.get(number.to_be_bytes().as_slice())? {
            Some(value) => Ok(Some(serde_json::from_slice(value.value())?)),
            None => Ok(None),
        }
    }

    fn load_blocks_range(&self, from: u64, to: u64) -> Result<Vec<Block>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BLOCKS)?;
        let mut blocks = Vec::new();
        for number in from..=to {
            if let Some(value) = table.get(number.to_be_bytes().as_slice())? {
                blocks.push(serde_json::from_slice(value.value())?);
            }
        }
        Ok(blocks)
    }

    fn record_height(&self, height: u64, state_root: B256) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut pruning_table = write_txn.open_table(PRUNING)?;
            pruning_table.insert(height.to_be_bytes().as_slice(), state_root.as_slice())?;
        }
        {
            let mut meta_table = write_txn.open_table(HEIGHT_META)?;
            meta_table.insert(b"latest_height".as_slice(), height.to_be_bytes().as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Interrupted-commit marker, mirroring the sled backend. `commit_state`
    // writes accounts, orders and `latest_height` in separate transactions,
    // and `store_block` follows in another: a hard kill in between leaves
    // `latest_height` ahead of the account writes or of the block record.
    // Without the marker the redb backend resumed from that state silently
    // and wedged on the next block (persisted h, block h missing → every
    // block h+1 fails the parent check; seen in the field on f67d812).
    fn begin_commit(&self, height: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut meta_table = write_txn.open_table(HEIGHT_META)?;
            meta_table.insert(
                b"commit_in_progress".as_slice(),
                height.to_be_bytes().as_slice(),
            )?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn end_commit(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut meta_table = write_txn.open_table(HEIGHT_META)?;
            meta_table.remove(b"commit_in_progress".as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn interrupted_commit(&self) -> Result<Option<u64>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(HEIGHT_META) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(b"commit_in_progress".as_slice())? {
            Some(value) if value.value().len() == 8 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(value.value());
                Ok(Some(u64::from_be_bytes(buf)))
            }
            _ => Ok(None),
        }
    }

    fn save_consensus_set(&self, set: &[(Address, U256)]) -> Result<()> {
        let mut buf = Vec::with_capacity(set.len() * 52);
        for (address, stake) in set {
            buf.extend_from_slice(address.as_slice());
            buf.extend_from_slice(&stake.to_be_bytes::<32>());
        }
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(HEIGHT_META)?;
            table.insert(b"consensus_set".as_slice(), buf.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn load_consensus_set(&self) -> Result<Option<Vec<(Address, U256)>>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(HEIGHT_META) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(b"consensus_set".as_slice())? {
            Some(v) if !v.value().is_empty() && v.value().len() % 52 == 0 => Ok(Some(
                v.value()
                    .chunks(52)
                    .map(|c| (Address::from_slice(&c[..20]), U256::from_be_slice(&c[20..])))
                    .collect(),
            )),
            _ => Ok(None),
        }
    }

    fn persisted_height(&self) -> Result<Option<u64>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(HEIGHT_META) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        match table.get(b"latest_height".as_slice())? {
            Some(value) if value.value().len() == 8 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(value.value());
                Ok(Some(u64::from_be_bytes(buf)))
            }
            _ => Ok(None),
        }
    }

    fn prune_before(&self, height: u64) -> Result<u64> {
        let mut pruned = 0u64;

        let write_txn = self.db.begin_write()?;
        {
            let mut pruning_table = write_txn.open_table(PRUNING)?;
            let to_remove: Vec<Vec<u8>> = {
                let iter = pruning_table.iter()?;
                iter.filter_map(|r| {
                    let (k, _) = r.ok()?;
                    let key_bytes = k.value();
                    if key_bytes.len() != 8 {
                        return None;
                    }
                    let mut buf = [0u8; 8];
                    buf.copy_from_slice(key_bytes);
                    let entry_height = u64::from_be_bytes(buf);
                    if entry_height < height {
                        Some(key_bytes.to_vec())
                    } else {
                        None
                    }
                })
                .collect()
            };
            for key in &to_remove {
                pruning_table.remove(key.as_slice())?;
                pruned += 1;
            }
        }
        {
            let mut blocks_table = write_txn.open_table(BLOCKS)?;
            let block_keys: Vec<Vec<u8>> = {
                let iter = blocks_table.iter()?;
                iter.filter_map(|r| {
                    let (k, _) = r.ok()?;
                    let key_bytes = k.value();
                    if key_bytes.len() != 8 {
                        return None;
                    }
                    let mut buf = [0u8; 8];
                    buf.copy_from_slice(key_bytes);
                    let block_number = u64::from_be_bytes(buf);
                    if block_number < height {
                        Some(key_bytes.to_vec())
                    } else {
                        None
                    }
                })
                .collect()
            };
            for key in &block_keys {
                blocks_table.remove(key.as_slice())?;
                pruned += 1;
            }
        }
        write_txn.commit()?;
        Ok(pruned)
    }

    fn generate_proof(&self, key: &[u8]) -> Result<StateProof> {
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

    fn export_snapshot_bytes(&self, height: u64, state_root: B256) -> Result<Vec<u8>> {
        let read_txn = self.db.begin_read()?;

        let mut accounts = Vec::new();
        let accounts_table = read_txn.open_table(ACCOUNTS)?;
        for entry in accounts_table.iter()? {
            let (key, value) = entry?;
            let record: AccountRecord = bincode::deserialize(value.value())?;
            accounts.push((key.value().to_vec(), record));
        }

        let mut storage = Vec::new();
        let storage_table = read_txn.open_table(STORAGE)?;
        for entry in storage_table.iter()? {
            let (key, value) = entry?;
            storage.push((key.value().to_vec(), value.value().to_vec()));
        }

        let (mersennet_orders, orders_extra) = {
            let table = read_txn.open_table(MERSENNET_ORDERS)?;
            let mut extra = Vec::new();
            for entry in table.iter()? {
                let (key, value) = entry?;
                if key.value() != b"state" {
                    extra.push((key.value().to_vec(), value.value().to_vec()));
                }
            }
            (
                table.get(b"state".as_slice())?.map(|v| v.value().to_vec()),
                extra,
            )
        };
        let validator_registry = match read_txn.open_table(HEIGHT_META) {
            Ok(meta) => meta
                .get(b"validator_registry".as_slice())?
                .map(|v| vec![(b"validator_registry".to_vec(), v.value().to_vec())])
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        let bridge_orders_to_evm = {
            let table = read_txn.open_table(BRIDGE_OTE)?;
            table.get(b"queue".as_slice())?.map(|v| v.value().to_vec())
        };

        let bridge_evm_to_orders = {
            let table = read_txn.open_table(BRIDGE_ETO)?;
            table.get(b"queue".as_slice())?.map(|v| v.value().to_vec())
        };

        let snapshot = SnapshotRecord {
            height,
            state_root: state_root.0,
            accounts,
            storage,
            mersennet_orders,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
            orders_extra,
            validator_registry,
        };
        Ok(bincode::serialize(&snapshot)?)
    }

    fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta> {
        let snapshot = SnapshotRecord::decode(data)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
            let keys: Vec<Vec<u8>> = {
                let iter = accounts_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                accounts_table.remove(key.as_slice())?;
            }
            for (key, record) in snapshot.accounts {
                let data = bincode::serialize(&record)?;
                accounts_table.insert(key.as_slice(), data.as_slice())?;
            }
        }
        {
            let mut storage_table = write_txn.open_table(STORAGE)?;
            let keys: Vec<Vec<u8>> = {
                let iter = storage_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                storage_table.remove(key.as_slice())?;
            }
            for (key, value) in snapshot.storage {
                storage_table.insert(key.as_slice(), value.as_slice())?;
            }
        }
        {
            let mut table = write_txn.open_table(MERSENNET_ORDERS)?;
            let keys: Vec<Vec<u8>> = {
                let iter = table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec()))
                    .collect()
            };
            for key in keys {
                table.remove(key.as_slice())?;
            }
            if let Some(data) = &snapshot.mersennet_orders {
                table.insert(b"state".as_slice(), data.as_slice())?;
            }
            for (key, value) in &snapshot.orders_extra {
                table.insert(key.as_slice(), value.as_slice())?;
            }
        }
        {
            let mut meta = write_txn.open_table(HEIGHT_META)?;
            meta.remove(b"validator_registry".as_slice())?;
            for (key, value) in &snapshot.validator_registry {
                meta.insert(key.as_slice(), value.as_slice())?;
            }
        }
        if let Some(data) = snapshot.bridge_orders_to_evm {
            let mut table = write_txn.open_table(BRIDGE_OTE)?;
            table.insert(b"queue".as_slice(), data.as_slice())?;
        }
        if let Some(data) = snapshot.bridge_evm_to_orders {
            let mut table = write_txn.open_table(BRIDGE_ETO)?;
            table.insert(b"queue".as_slice(), data.as_slice())?;
        }
        write_txn.commit()?;

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
}
