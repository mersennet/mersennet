use anyhow::{bail, Result};
use revm::db::InMemoryDB;
use revm::primitives::{keccak256, AccountInfo, Address, Bytes, Bytecode, B256, U256};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::collections::HashMap;

use crate::bridge::{BridgeDomain, BridgeMessage, BridgeQueue, BridgeQueueSnapshot};
use crate::prime_orders::{AccountState, Market, MarketId, Order, OrderId, OrderBook, Position, PrimeOrdersState, Side, TimeInForce};

#[derive(Debug)]
pub struct PersistentState {
    db: sled::Db,
    accounts: sled::Tree,
    storage: sled::Tree,
    prime_orders: sled::Tree,
    bridge_orders_to_evm: sled::Tree,
    bridge_evm_to_orders: sled::Tree,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AccountRecord {
    balance: [u8; 32],
    nonce: u64,
    code_hash: [u8; 32],
    code: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotRecord {
    height: u64,
    state_root: [u8; 32],
    accounts: Vec<(Vec<u8>, AccountRecord)>,
    storage: Vec<(Vec<u8>, Vec<u8>)>,
    prime_orders: Option<Vec<u8>>,
    bridge_orders_to_evm: Option<Vec<u8>>,
    bridge_evm_to_orders: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PrimeOrdersSnapshot {
    next_order_id: u64,
    initial_margin_bps: u64,
    maintenance_margin_bps: u64,
    markets: Vec<MarketRecord>,
    orders: Vec<OrderRecord>,
    accounts: Vec<(Vec<u8>, AccountRecordV2)>,
    books: Vec<(u64, OrderBookRecord)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MarketRecord {
    id: u64,
    symbol: String,
    tick_size: [u8; 32],
    lot_size: [u8; 32],
    last_price: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderRecord {
    id: u64,
    owner: Vec<u8>,
    market: u64,
    side: u8,
    price: [u8; 32],
    size: [u8; 32],
    tif: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PositionRecord {
    size: i128,
    entry_price: [u8; 32],
    realized_pnl: i128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AccountRecordV2 {
    collateral: [u8; 32],
    open_orders: Vec<u64>,
    positions: Vec<(u64, PositionRecord)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderBookRecord {
    bids: Vec<(Vec<u8>, Vec<u64>)>,
    asks: Vec<(Vec<u8>, Vec<u64>)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BridgeQueueRecord {
    next_nonce: u64,
    messages: Vec<BridgeMessageRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BridgeMessageRecord {
    nonce: u64,
    from: u8,
    to: u8,
    payload: Vec<u8>,
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
        let prime_orders = db.open_tree("prime_orders")?;
        let bridge_orders_to_evm = db.open_tree("bridge_orders_to_evm")?;
        let bridge_evm_to_orders = db.open_tree("bridge_evm_to_orders")?;
        Ok(Self {
            db,
            accounts,
            storage,
            prime_orders,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
        })
    }

    pub fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()> {
        for entry in self.accounts.iter() {
            let (key, value) = entry?;
            let address = Address::from_slice(&key);
            let record: AccountRecord = bincode::deserialize(&value)?;
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
        prime_orders: &PrimeOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
    ) -> Result<B256> {
        self.write_evm_state(evm_db)?;
        self.commit_prime_orders(prime_orders)?;
        self.commit_bridge_queues(bridge_orders_to_evm, bridge_evm_to_orders)?;
        Ok(self.compute_state_root())
    }

    fn write_evm_state(&self, evm_db: &InMemoryDB) -> Result<()> {
        self.accounts.clear()?;
        self.storage.clear()?;

        for (address, db_account) in &evm_db.accounts {
            if let Some(info) = db_account.info() {
                let record = AccountRecord {
                    balance: info.balance.to_be_bytes(),
                    nonce: info.nonce,
                    code_hash: info.code_hash.into(),
                    code: info.code.map(|code| code.bytes().to_vec()).unwrap_or_default(),
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

    pub fn load_prime_orders(&self, state: &mut PrimeOrdersState) -> Result<()> {
        let Some(value) = self.prime_orders.get("state")? else {
            return Ok(());
        };
        let snapshot: PrimeOrdersSnapshot = bincode::deserialize(&value)?;
        state.next_order_id = snapshot.next_order_id;
        state.initial_margin_bps = snapshot.initial_margin_bps;
        state.maintenance_margin_bps = snapshot.maintenance_margin_bps;

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
                book.bids.insert(price, orders.into_iter().map(OrderId).collect());
            }
            for (price_bytes, orders) in record.asks {
                let price = U256::from_be_bytes(bytes_to_u256(&price_bytes)?);
                book.asks.insert(price, orders.into_iter().map(OrderId).collect());
            }
            state.books.insert(MarketId(market_id), book);
        }

        Ok(())
    }

    pub fn commit_prime_orders(&self, state: &PrimeOrdersState) -> Result<()> {
        self.prime_orders.clear()?;
        let markets = state
            .markets
            .values()
            .map(|m| MarketRecord {
                id: m.id.0,
                symbol: m.symbol.clone(),
                tick_size: m.tick_size.to_be_bytes(),
                lot_size: m.lot_size.to_be_bytes(),
                last_price: m.last_price.to_be_bytes(),
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
                        (price.to_be_bytes::<32>().to_vec(), orders.iter().map(|id| id.0).collect())
                    })
                    .collect();
                let asks = book
                    .asks
                    .iter()
                    .map(|(price, orders)| {
                        (price.to_be_bytes::<32>().to_vec(), orders.iter().map(|id| id.0).collect())
                    })
                    .collect();
                (market_id.0, OrderBookRecord { bids, asks })
            })
            .collect();

        let snapshot = PrimeOrdersSnapshot {
            next_order_id: state.next_order_id,
            initial_margin_bps: state.initial_margin_bps,
            maintenance_margin_bps: state.maintenance_margin_bps,
            markets,
            orders,
            accounts,
            books,
        };

        let data = bincode::serialize(&snapshot)?;
        self.prime_orders.insert("state", data)?;
        self.db.flush()?;
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
            prime_orders: self.prime_orders.get("state")?.map(|v| v.to_vec()),
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

        if let Some(data) = snapshot.prime_orders {
            self.prime_orders.insert("state", data)?;
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

    pub fn compute_state_root(&self) -> B256 {
        let mut items = Vec::new();

        for entry in self.accounts.iter() {
            if let Ok((key, value)) = entry {
                items.push((key.to_vec(), value.to_vec()));
            }
        }

        for entry in self.storage.iter() {
            if let Ok((key, value)) = entry {
                items.push((key.to_vec(), value.to_vec()));
            }
        }

        for entry in self.prime_orders.iter() {
            if let Ok((key, value)) = entry {
                items.push((key.to_vec(), value.to_vec()));
            }
        }

        for entry in self.bridge_orders_to_evm.iter() {
            if let Ok((key, value)) = entry {
                items.push((key.to_vec(), value.to_vec()));
            }
        }

        for entry in self.bridge_evm_to_orders.iter() {
            if let Ok((key, value)) = entry {
                items.push((key.to_vec(), value.to_vec()));
            }
        }

        items.sort_by(|a, b| a.0.cmp(&b.0));
        let mut buffer = Vec::new();
        for (key, value) in items {
            buffer.extend_from_slice(&key);
            buffer.extend_from_slice(&value);
        }

        keccak256(buffer)
    }
}

fn encode_side(side: Side) -> u8 {
    match side {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

fn decode_side(value: u8) -> Result<Side> {
    match value {
        0 => Ok(Side::Buy),
        1 => Ok(Side::Sell),
        _ => bail!("invalid side"),
    }
}

fn encode_tif(tif: TimeInForce) -> u8 {
    match tif {
        TimeInForce::Gtc => 0,
        TimeInForce::Ioc => 1,
        TimeInForce::Fok => 2,
    }
}

fn decode_tif(value: u8) -> Result<TimeInForce> {
    match value {
        0 => Ok(TimeInForce::Gtc),
        1 => Ok(TimeInForce::Ioc),
        2 => Ok(TimeInForce::Fok),
        _ => bail!("invalid tif"),
    }
}

fn bytes_to_u256(bytes: &[u8]) -> Result<[u8; 32]> {
    if bytes.len() != 32 {
        bail!("invalid U256 bytes length");
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn encode_bridge_queue(snapshot: BridgeQueueSnapshot) -> BridgeQueueRecord {
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

fn decode_bridge_queue(record: BridgeQueueRecord) -> Result<BridgeQueueSnapshot> {
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

fn encode_domain(domain: BridgeDomain) -> u8 {
    match domain {
        BridgeDomain::PrimeOrders => 0,
        BridgeDomain::PrimeEvm => 1,
    }
}

fn decode_domain(value: u8) -> Result<BridgeDomain> {
    match value {
        0 => Ok(BridgeDomain::PrimeOrders),
        1 => Ok(BridgeDomain::PrimeEvm),
        _ => bail!("invalid bridge domain"),
    }
}
