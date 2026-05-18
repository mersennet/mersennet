use anyhow::Result;
use revm::db::InMemoryDB;
use revm::primitives::{Address, B256};

use crate::bridge::BridgeQueue;
use crate::engine::Block;
use crate::prime_orders::PrimeOrdersState;
use crate::state::{SnapshotMeta, StateProof};

pub trait StateBackend: Send + std::fmt::Debug {
    fn mark_dirty(&self, address: Address);
    fn load_into_db(&self, evm_db: &mut InMemoryDB) -> Result<()>;

    fn commit_state(
        &self,
        evm_db: &InMemoryDB,
        prime_orders: &PrimeOrdersState,
        bridge_orders_to_evm: &BridgeQueue,
        bridge_evm_to_orders: &BridgeQueue,
        height: u64,
    ) -> Result<B256>;

    fn load_prime_orders(&self, state: &mut PrimeOrdersState) -> Result<()>;
    fn commit_prime_orders(&self, state: &PrimeOrdersState) -> Result<()>;

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

    fn record_height(&self, height: u64, state_root: B256) -> Result<()>;
    fn prune_before(&self, height: u64) -> Result<u64>;

    fn generate_proof(&self, key: &[u8]) -> Result<StateProof>;

    fn export_snapshot_bytes(&self, height: u64, state_root: B256) -> Result<Vec<u8>>;
    fn import_snapshot_bytes(&self, data: &[u8]) -> Result<SnapshotMeta>;
}
