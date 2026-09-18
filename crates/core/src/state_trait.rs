use anyhow::Result;
use revm::db::InMemoryDB;
use revm::primitives::{Address, B256, U256};

use crate::bridge::BridgeQueue;
use crate::engine::Block;
use crate::mersennet_orders::MersennetOrdersState;
use crate::state::{SnapshotMeta, StateProof};

pub trait StateBackend: Send + std::fmt::Debug {
    fn mark_dirty(&self, address: Address);
    fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()>;

    fn commit_state(
        &self,
        evm_db: &InMemoryDB,
        mersennet_orders: &MersennetOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
        height: u64,
    ) -> Result<B256>;

    fn load_mersennet_orders(&self, state: &mut MersennetOrdersState) -> Result<()>;
    fn commit_mersennet_orders(&self, state: &MersennetOrdersState) -> Result<()>;

    fn load_bridge_queues(
        &self,
        orders_to_evm: &mut BridgeQueue,
        evm_to_orders: &mut BridgeQueue,
    ) -> Result<()>;

    fn commit_bridge_queues(
        &self,
        orders_to_evm: &BridgeQueue,
        evm_to_orders: &BridgeQueue,
    ) -> Result<()>;

    fn compute_state_root(&self) -> B256;

    fn store_block(&self, block: &Block) -> Result<()>;
    fn load_block(&self, number: u64) -> Result<Option<Block>>;
    fn load_blocks_range(&self, from: u64, to: u64) -> Result<Vec<Block>>;

    /// History index (block hash → height, tx hash → height + index) so the
    /// RPC can serve `eth_getBlockByHash`, `eth_getTransactionByHash` and
    /// `eth_getTransactionReceipt` for blocks outside the in-memory window.
    /// `store_block` writes it for new blocks; `index_block` is the backfill
    /// path for blocks stored before the index existed.
    fn index_block(&self, _block: &Block) -> Result<()> {
        Ok(())
    }
    fn block_number_by_hash(&self, _hash: B256) -> Result<Option<u64>> {
        Ok(None)
    }
    fn tx_location(&self, _hash: B256) -> Result<Option<(u64, u32)>> {
        Ok(None)
    }
    /// Lowest height known to be indexed (`None`: backfill not started).
    fn history_index_floor(&self) -> Result<Option<u64>> {
        Ok(None)
    }
    fn set_history_index_floor(&self, _height: u64) -> Result<()> {
        Ok(())
    }

    fn record_height(&self, height: u64, state_root: B256) -> Result<()>;
    /// Durability marker around a block commit (account state, orders,
    /// height, block are separate writes). Set before the first write and
    /// cleared after the last; a marker still present at startup means a
    /// previous commit was interrupted and the persisted state may be ahead
    /// of the recorded height.
    fn begin_commit(&self, _height: u64) -> Result<()> {
        Ok(())
    }
    fn end_commit(&self) -> Result<()> {
        Ok(())
    }
    fn interrupted_commit(&self) -> Result<Option<u64>> {
        Ok(None)
    }
    /// Make all prior writes durable (called before a process exit).
    fn flush(&self) -> Result<()> {
        Ok(())
    }
    /// The highest block height committed to this store, if any. Used
    /// on startup to resume the chain at the persisted height instead
    /// of re-producing from genesis.
    fn persisted_height(&self) -> Result<Option<u64>>;
    fn prune_before(&self, height: u64) -> Result<u64>;
    /// Node-local cache of the consensus set installed at the last epoch
    /// transition, restored on startup (not part of the state root).
    fn save_consensus_set(&self, _set: &[(Address, U256)]) -> Result<()> {
        Ok(())
    }
    fn load_consensus_set(&self) -> Result<Option<Vec<(Address, U256)>>> {
        Ok(None)
    }

    fn generate_proof(&self, key: &[u8]) -> Result<StateProof>;

    fn export_snapshot_bytes(&self, height: u64, state_root: B256) -> Result<Vec<u8>>;
    fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta>;
}
