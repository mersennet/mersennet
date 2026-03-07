use anyhow::{bail, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use revm::db::InMemoryDB;
use revm::primitives::{keccak256, AccountInfo, Address, Bytes, Bytecode, B256, U256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use crate::bridge::BridgeQueue;
use crate::engine::Block;
use crate::prime_orders::{
    AccountState, Market, MarketId, Order, OrderBook, OrderId, Position,
    PrimeOrdersState,
};
use crate::state::{
    AccountRecord, AccountRecordV2, BridgeQueueRecord, MarketRecord,
    MerkleTree, OrderBookRecord, OrderRecord, PositionRecord, PrimeOrdersSnapshot, SnapshotMeta,
    SnapshotRecord, StateProof,
    bytes_to_u256, decode_bridge_queue, decode_market_status, decode_side, decode_tif,
    encode_bridge_queue, encode_market_status, encode_side, encode_tif,
};
use crate::state_trait::StateBackend;

const ACCOUNTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("accounts");
const STORAGE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("storage");
const PRIME_ORDERS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("prime_orders");
const BRIDGE_OTE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bridge_orders_to_evm");
const BRIDGE_ETO: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bridge_evm_to_orders");
const BLOCKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blocks");
const PRUNING: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pruning");
const HEIGHT_META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("height_meta");

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
        let db_path = path.as_ref().join("prime_chain.redb");
        let db = Database::create(&db_path)?;

        {
            let write_txn = db.begin_write()?;
            let _ = write_txn.open_table(ACCOUNTS)?;
            let _ = write_txn.open_table(STORAGE)?;
            let _ = write_txn.open_table(PRIME_ORDERS)?;
            let _ = write_txn.open_table(BRIDGE_OTE)?;
            let _ = write_txn.open_table(BRIDGE_ETO)?;
            let _ = write_txn.open_table(BLOCKS)?;
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
        let dirty = {
            let mut guard = self.dirty_accounts.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        let write_txn = self.db.begin_write()?;

        if dirty.is_empty() {
            {
                let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
                let keys: Vec<Vec<u8>> = {
                    let iter = accounts_table.iter()?;
                    iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
                };
                for key in keys {
                    accounts_table.remove(key.as_slice())?;
                }
            }
            {
                let mut storage_table = write_txn.open_table(STORAGE)?;
                let keys: Vec<Vec<u8>> = {
                    let iter = storage_table.iter()?;
                    iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
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
                            code: info.code.map(|code| code.bytes().to_vec()).unwrap_or_default(),
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
                        storage_table
                            .insert(key.as_slice(), value.to_be_bytes::<32>().as_slice())?;
                    }
                }
            }
        } else {
            let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
            let mut storage_table = write_txn.open_table(STORAGE)?;

            for address in &dirty {
                if let Some(db_account) = evm_db.accounts.get(address) {
                    if let Some(info) = db_account.info() {
                        let record = AccountRecord {
                            balance: info.balance.to_be_bytes(),
                            nonce: info.nonce,
                            code_hash: info.code_hash.into(),
                            code: info
                                .code
                                .map(|code| code.bytes().to_vec())
                                .unwrap_or_default(),
                        };
                        let data = bincode::serialize(&record)?;
                        accounts_table.insert(address.as_slice(), data.as_slice())?;
                    }

                    let prefix = address.as_slice();
                    let range_start = {
                        let mut start = [0u8; 52];
                        start[..20].copy_from_slice(prefix);
                        start
                    };
                    let range_end = {
                        let mut end = [0u8; 52];
                        end[..20].copy_from_slice(prefix);
                        end[19] = end[19].wrapping_add(1);
                        if end[19] == 0 {
                            for i in (0..19).rev() {
                                end[i] = end[i].wrapping_add(1);
                                if end[i] != 0 {
                                    break;
                                }
                            }
                        }
                        end
                    };

                    let old_keys: Vec<Vec<u8>> = {
                        let range = storage_table.range(range_start.as_slice()..range_end.as_slice())?;
                        range.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
                    };
                    for key in old_keys {
                        storage_table.remove(key.as_slice())?;
                    }

                    for (slot, value) in &db_account.storage {
                        if value.is_zero() {
                            continue;
                        }
                        let mut key = [0u8; 52];
                        key[..20].copy_from_slice(address.as_slice());
                        key[20..52].copy_from_slice(&slot.to_be_bytes::<32>());
                        storage_table
                            .insert(key.as_slice(), value.to_be_bytes::<32>().as_slice())?;
                    }
                } else {
                    accounts_table.remove(address.as_slice())?;

                    let prefix = address.as_slice();
                    let range_start = {
                        let mut start = [0u8; 52];
                        start[..20].copy_from_slice(prefix);
                        start
                    };
                    let range_end = {
                        let mut end = [0u8; 52];
                        end[..20].copy_from_slice(prefix);
                        end[19] = end[19].wrapping_add(1);
                        if end[19] == 0 {
                            for i in (0..19).rev() {
                                end[i] = end[i].wrapping_add(1);
                                if end[i] != 0 {
                                    break;
                                }
                            }
                        }
                        end
                    };

                    let old_keys: Vec<Vec<u8>> = {
                        let range = storage_table.range(range_start.as_slice()..range_end.as_slice())?;
                        range.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
                    };
                    for key in old_keys {
                        storage_table.remove(key.as_slice())?;
                    }
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

        for table_def in [ACCOUNTS, STORAGE, PRIME_ORDERS, BRIDGE_OTE, BRIDGE_ETO] {
            if let Ok(table) = read_txn.open_table(table_def) {
                if let Ok(iter) = table.iter() {
                    for entry in iter {
                        if let Ok((key, value)) = entry {
                            items.push((key.value().to_vec(), value.value().to_vec()));
                        }
                    }
                }
            }
        }
        items
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
            let code = if record.code.is_empty() {
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
        prime_orders: &PrimeOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
        height: u64,
    ) -> Result<B256> {
        self.write_evm_state(evm_db)?;
        self.commit_prime_orders(prime_orders)?;
        self.commit_bridge_queues(bridge_orders_to_evm, bridge_evm_to_orders)?;
        let root = self.compute_state_root();
        self.record_height(height, root)?;
        Ok(root)
    }

    fn load_prime_orders(&self, state: &mut PrimeOrdersState) -> Result<()> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PRIME_ORDERS)?;
        let Some(value) = table.get(b"state".as_slice())? else {
            return Ok(());
        };
        let snapshot: PrimeOrdersSnapshot = bincode::deserialize(value.value())?;
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

        Ok(())
    }

    fn commit_prime_orders(&self, state: &PrimeOrdersState) -> Result<()> {
        let markets = state
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
        let orders = state
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
            })
            .collect();
        let accounts = state
            .accounts
            .iter()
            .map(|(addr, acct)| {
                let positions = acct
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
                (
                    addr.as_slice().to_vec(),
                    AccountRecordV2 {
                        collateral: acct.collateral.to_be_bytes(),
                        open_orders: acct.open_orders.iter().map(|id| id.0).collect(),
                        positions,
                    },
                )
            })
            .collect();
        let books = state
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

        let snapshot = PrimeOrdersSnapshot {
            next_order_id: state.next_order_id,
            initial_margin_bps: state.initial_margin_bps,
            maintenance_margin_bps: state.maintenance_margin_bps,
            insurance_fund: state.insurance_fund.to_be_bytes(),
            insurance_contribution_rate_bps: state.insurance_contribution_rate_bps,
            markets,
            orders,
            accounts,
            books,
        };

        let data = bincode::serialize(&snapshot)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PRIME_ORDERS)?;
            table.remove(b"state".as_slice())?;
            table.insert(b"state".as_slice(), data.as_slice())?;
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
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
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
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
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

        let prime_orders = {
            let table = read_txn.open_table(PRIME_ORDERS)?;
            table
                .get(b"state".as_slice())?
                .map(|v| v.value().to_vec())
        };

        let bridge_orders_to_evm = {
            let table = read_txn.open_table(BRIDGE_OTE)?;
            table
                .get(b"queue".as_slice())?
                .map(|v| v.value().to_vec())
        };

        let bridge_evm_to_orders = {
            let table = read_txn.open_table(BRIDGE_ETO)?;
            table
                .get(b"queue".as_slice())?
                .map(|v| v.value().to_vec())
        };

        let snapshot = SnapshotRecord {
            height,
            state_root: state_root.0,
            accounts,
            storage,
            prime_orders,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
        };
        Ok(bincode::serialize(&snapshot)?)
    }

    fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta> {
        let snapshot: SnapshotRecord = bincode::deserialize(data)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut accounts_table = write_txn.open_table(ACCOUNTS)?;
            let keys: Vec<Vec<u8>> = {
                let iter = accounts_table.iter()?;
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
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
                iter.filter_map(|r| r.ok().map(|(k, _)| k.value().to_vec())).collect()
            };
            for key in keys {
                storage_table.remove(key.as_slice())?;
            }
            for (key, value) in snapshot.storage {
                storage_table.insert(key.as_slice(), value.as_slice())?;
            }
        }
        if let Some(data) = snapshot.prime_orders {
            let mut table = write_txn.open_table(PRIME_ORDERS)?;
            table.insert(b"state".as_slice(), data.as_slice())?;
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
