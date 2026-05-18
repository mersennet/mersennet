use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use revm::db::InMemoryDB;
use revm::primitives::{
    AccountInfo, Address, Bytes, Env, ExecutionResult, SpecId, TxKind, U256,
};
use revm::Evm;

use crate::engine::{LogEntry, Transaction, TxExecution};

/// Below this tx count, sequential execution is faster due to parallelism overhead.
const PARALLEL_THRESHOLD: usize = 8;

/// Parallel EVM executor using optimistic concurrency control (Block-STM / Grevm pattern).
///
/// Statically analyzes transaction dependencies, groups non-conflicting transactions,
/// executes groups in parallel on forked DB snapshots, validates via MVCC, and merges
/// the resulting states. Falls back to sequential execution on conflict or small batches.
pub struct ParallelExecutor {
    pub num_threads: usize,
    pub max_retries: usize,
}

/// Read/write address sets for a single transaction, used for conflict detection.
#[derive(Clone, Debug, Default)]
pub struct TxAccessSet {
    pub reads: HashSet<Address>,
    pub writes: HashSet<Address>,
}

/// Multi-version concurrency control memory for speculative execution and validation.
///
/// Stores multiple versions of account state keyed by `(address, tx_index)`, enabling
/// optimistic reads and post-execution conflict detection per the Block-STM protocol.
pub struct MultiVersionMemory {
    versions: HashMap<Address, BTreeMap<usize, AccountInfo>>,
}

impl MultiVersionMemory {
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    /// Read the latest version written by a tx with index strictly less than `tx_index`.
    pub fn read(&self, address: Address, tx_index: usize) -> Option<AccountInfo> {
        self.versions
            .get(&address)?
            .range(..tx_index)
            .next_back()
            .map(|(_, info)| info.clone())
    }

    /// Record a speculative write for `tx_index`.
    pub fn write(&mut self, address: Address, tx_index: usize, info: AccountInfo) {
        self.versions
            .entry(address)
            .or_default()
            .insert(tx_index, info);
    }

    /// Validate that values in the read set of `tx_index` haven't been modified by a
    /// lower-indexed tx relative to the base DB state. Returns `false` on conflict.
    pub fn validate(
        &self,
        tx_index: usize,
        read_set: &TxAccessSet,
        base_db: &InMemoryDB,
    ) -> bool {
        for addr in &read_set.reads {
            let mv_version = self.read(*addr, tx_index);
            let base_info = base_db.accounts.get(addr).and_then(|a| a.info());
            match (mv_version, base_info) {
                (Some(mv), Some(base)) => {
                    if mv.balance != base.balance || mv.nonce != base.nonce {
                        return false;
                    }
                }
                (Some(_), None) => return false,
                (None, _) => {}
            }
        }
        true
    }
}

/// Static dependency analysis: determine which addresses each tx reads/writes.
/// Coinbase is intentionally excluded to avoid false dependencies (every tx pays
/// gas to coinbase, which would collapse all groups into one). Coinbase balance
/// deltas are accumulated separately during the merge phase.
pub fn analyze_dependencies(txs: &[Transaction]) -> Vec<TxAccessSet> {
    txs.iter()
        .map(|tx| {
            let mut access = TxAccessSet::default();
            access.reads.insert(tx.from);
            access.writes.insert(tx.from);
            if let Some(to) = tx.to {
                access.reads.insert(to);
                access.writes.insert(to);
            }
            access
        })
        .collect()
}

/// Disjoint-set (union-find) with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

/// Group transactions into independent sets using union-find over read/write conflicts.
/// Transactions within the same group must execute sequentially; different groups can
/// execute in parallel since they have no overlapping write addresses.
pub fn find_independent_groups(access_sets: &[TxAccessSet]) -> Vec<Vec<usize>> {
    let n = access_sets.len();
    if n == 0 {
        return Vec::new();
    }

    let mut uf = UnionFind::new(n);

    let mut write_map: HashMap<Address, Vec<usize>> = HashMap::new();
    for (i, access) in access_sets.iter().enumerate() {
        for addr in &access.writes {
            write_map.entry(*addr).or_default().push(i);
        }
    }

    for (addr, writers) in &write_map {
        for w in 1..writers.len() {
            uf.union(writers[0], writers[w]);
        }
        for (i, access) in access_sets.iter().enumerate() {
            if access.reads.contains(addr) && !writers.contains(&i) {
                uf.union(writers[0], i);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(uf.find(i)).or_default().push(i);
    }

    let mut result: Vec<Vec<usize>> = groups.into_values().collect();
    result.sort_by_key(|g| g[0]);
    result
}

/// Output of a parallel (or sequential-fallback) block execution.
pub struct ParallelExecutionResult {
    /// Per-tx results sorted by original tx index.
    pub results: Vec<(usize, TxExecution)>,
    /// Merged DB containing all state changes from every executed tx.
    pub merged_db: InMemoryDB,
    /// All addresses whose state was modified.
    pub dirty_addresses: HashSet<Address>,
}

struct GroupResult {
    executions: Vec<(usize, TxExecution)>,
    db_snapshot: InMemoryDB,
    dirty_addresses: HashSet<Address>,
}

fn build_tx_env(
    tx: &Transaction,
    chain_id: u64,
    block_number: u64,
    coinbase: Address,
    gas_limit_per_block: u64,
    base_fee: U256,
) -> Env {
    let mut env = Env::default();
    env.cfg.chain_id = chain_id;
    env.block.number = U256::from(block_number);
    env.block.coinbase = coinbase;
    env.block.gas_limit = U256::from(gas_limit_per_block);
    env.block.basefee = base_fee;
    env.tx.caller = tx.from;
    env.tx.gas_limit = tx.gas_limit;
    env.tx.gas_price = tx.gas_price;
    env.tx.nonce = Some(tx.nonce);
    env.tx.chain_id = Some(tx.chain_id.unwrap_or(chain_id));
    env.tx.value = tx.value;
    env.tx.data = tx.data.clone();
    env.tx.transact_to = match tx.to {
        Some(to) => TxKind::Call(to),
        None => TxKind::Create,
    };
    env
}

/// Execute a single transaction on a forked DB, returning the result and mutating the DB
/// in place to reflect the post-execution state.
fn execute_tx_on_fork(
    _tx: &Transaction,
    db: &mut InMemoryDB,
    spec_id: SpecId,
    env: Env,
) -> anyhow::Result<TxExecution> {
    let mut revm_instance = Evm::builder()
        .with_db(db.clone())
        .with_spec_id(spec_id)
        .with_env(Box::new(env))
        .build();

    let result = revm_instance.transact_commit()?;
    *db = std::mem::take(&mut revm_instance.context.evm.db);

    let execution = match result {
        ExecutionResult::Success {
            gas_used,
            output,
            logs,
            ..
        } => TxExecution {
            success: true,
            gas_used,
            output: output.clone().into_data(),
            created_address: output.address().cloned(),
            logs: logs
                .into_iter()
                .map(|log| LogEntry {
                    address: log.address,
                    topics: log.data.topics().to_vec(),
                    data: log.data.data.clone(),
                })
                .collect(),
        },
        ExecutionResult::Revert { gas_used, output } => TxExecution {
            success: false,
            gas_used,
            output,
            created_address: None,
            logs: Vec::new(),
        },
        ExecutionResult::Halt { gas_used, .. } => TxExecution {
            success: false,
            gas_used,
            output: Bytes::new(),
            created_address: None,
            logs: Vec::new(),
        },
    };

    Ok(execution)
}

/// Merge forked DBs from independent groups into a single DB.
///
/// Each group wrote to a disjoint set of addresses (except coinbase), so we take
/// each group's version of its dirty accounts directly. Coinbase balance deltas
/// are accumulated across all groups to produce the correct aggregate.
fn merge_fork_dbs(
    base: &InMemoryDB,
    groups: &[GroupResult],
    coinbase: Address,
) -> InMemoryDB {
    let mut merged = base.clone();

    for group in groups {
        for addr in &group.dirty_addresses {
            if *addr == coinbase {
                continue;
            }
            if let Some(db_account) = group.db_snapshot.accounts.get(addr) {
                merged.accounts.insert(*addr, db_account.clone());
            }
        }
    }

    let base_coinbase_balance = base
        .accounts
        .get(&coinbase)
        .and_then(|a| a.info())
        .map(|i| i.balance)
        .unwrap_or_default();

    let mut total_delta = U256::ZERO;
    for group in groups {
        let group_balance = group
            .db_snapshot
            .accounts
            .get(&coinbase)
            .and_then(|a| a.info())
            .map(|i| i.balance)
            .unwrap_or_default();
        total_delta =
            total_delta.saturating_add(group_balance.saturating_sub(base_coinbase_balance));
    }

    let mut coinbase_info = merged
        .accounts
        .get(&coinbase)
        .and_then(|a| a.info())
        .unwrap_or_default();
    coinbase_info.balance = base_coinbase_balance.saturating_add(total_delta);
    merged.insert_account_info(coinbase, coinbase_info);

    merged
}

impl ParallelExecutor {
    pub fn new(num_threads: usize, max_retries: usize) -> Self {
        Self {
            num_threads: num_threads.max(1),
            max_retries,
        }
    }

    /// Execute a batch of transactions using optimistic parallel execution.
    ///
    /// 1. Analyze dependencies (read/write sets per tx)
    /// 2. Group independent transactions via union-find
    /// 3. Execute each group in parallel on a forked DB snapshot (std::thread::scope)
    /// 4. Validate via MVCC that no cross-group conflicts occurred
    /// 5. On conflict, fall back to sequential re-execution
    /// 6. Merge group DB snapshots into a single result
    pub fn execute(
        &self,
        txs: &[Transaction],
        db: &InMemoryDB,
        chain_id: u64,
        block_number: u64,
        coinbase: Address,
        gas_limit_per_block: u64,
        base_fee: U256,
        spec_id: SpecId,
    ) -> ParallelExecutionResult {
        if txs.len() < PARALLEL_THRESHOLD {
            return self.execute_sequential(
                txs,
                db,
                chain_id,
                block_number,
                coinbase,
                gas_limit_per_block,
                base_fee,
                spec_id,
            );
        }

        let access_sets = analyze_dependencies(txs);
        let groups = find_independent_groups(&access_sets);

        if groups.len() <= 1 {
            return self.execute_sequential(
                txs,
                db,
                chain_id,
                block_number,
                coinbase,
                gas_limit_per_block,
                base_fee,
                spec_id,
            );
        }

        tracing::info!(
            groups = groups.len(),
            txs = txs.len(),
            "parallel EVM execution"
        );

        let mvcc = Arc::new(Mutex::new(MultiVersionMemory::new()));

        let group_results: Vec<GroupResult> = std::thread::scope(|s| {
            let handles: Vec<_> = groups
                .iter()
                .map(|group| {
                    let mut fork_db = db.clone();
                    let group = group.clone();
                    let mvcc = Arc::clone(&mvcc);

                    s.spawn(move || {
                        let mut executions = Vec::new();
                        let mut dirty = HashSet::new();

                        for &tx_idx in &group {
                            let tx = &txs[tx_idx];
                            let env = build_tx_env(
                                tx,
                                chain_id,
                                block_number,
                                coinbase,
                                gas_limit_per_block,
                                base_fee,
                            );

                            match execute_tx_on_fork(tx, &mut fork_db, spec_id, env) {
                                Ok(exec) => {
                                    dirty.insert(tx.from);
                                    if let Some(to) = tx.to {
                                        dirty.insert(to);
                                    }
                                    if let Some(addr) = &exec.created_address {
                                        dirty.insert(*addr);
                                    }
                                    dirty.insert(coinbase);

                                    if let Ok(mut mv) = mvcc.lock() {
                                        let mut addrs = vec![tx.from];
                                        if let Some(to) = tx.to {
                                            addrs.push(to);
                                        }
                                        for addr in addrs {
                                            if let Some(acct) =
                                                fork_db.accounts.get(&addr)
                                            {
                                                if let Some(info) = acct.info() {
                                                    mv.write(addr, tx_idx, info);
                                                }
                                            }
                                        }
                                    }

                                    executions.push((tx_idx, exec));
                                }
                                Err(_) => {
                                    executions.push((
                                        tx_idx,
                                        TxExecution {
                                            success: false,
                                            gas_used: 0,
                                            output: Bytes::new(),
                                            created_address: None,
                                            logs: Vec::new(),
                                        },
                                    ));
                                }
                            }
                        }

                        GroupResult {
                            executions,
                            db_snapshot: fork_db,
                            dirty_addresses: dirty,
                        }
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|h| h.join().expect("parallel executor worker panicked"))
                .collect()
        });

        // MVCC validation as safety net (static grouping should prevent conflicts)
        let mv = mvcc.lock().unwrap();
        let has_conflict = groups.iter().any(|group| {
            group
                .iter()
                .any(|&tx_idx| !mv.validate(tx_idx, &access_sets[tx_idx], db))
        });
        drop(mv);

        if has_conflict {
            tracing::warn!("parallel conflict detected, sequential fallback");
            return self.execute_sequential(
                txs,
                db,
                chain_id,
                block_number,
                coinbase,
                gas_limit_per_block,
                base_fee,
                spec_id,
            );
        }

        let mut all_dirty = HashSet::new();
        for gr in &group_results {
            all_dirty.extend(&gr.dirty_addresses);
        }

        let merged_db = merge_fork_dbs(db, &group_results, coinbase);

        let mut all_results: Vec<(usize, TxExecution)> = group_results
            .into_iter()
            .flat_map(|gr| gr.executions)
            .collect();
        all_results.sort_by_key(|(idx, _)| *idx);

        metrics::increment_counter!("parallel_execution_total");
        metrics::gauge!("parallel_execution_groups", groups.len() as f64);

        ParallelExecutionResult {
            results: all_results,
            merged_db,
            dirty_addresses: all_dirty,
        }
    }

    fn execute_sequential(
        &self,
        txs: &[Transaction],
        db: &InMemoryDB,
        chain_id: u64,
        block_number: u64,
        coinbase: Address,
        gas_limit_per_block: u64,
        base_fee: U256,
        spec_id: SpecId,
    ) -> ParallelExecutionResult {
        let mut fork_db = db.clone();
        let mut results = Vec::with_capacity(txs.len());
        let mut dirty = HashSet::new();

        for (i, tx) in txs.iter().enumerate() {
            let env = build_tx_env(
                tx,
                chain_id,
                block_number,
                coinbase,
                gas_limit_per_block,
                base_fee,
            );

            match execute_tx_on_fork(tx, &mut fork_db, spec_id, env) {
                Ok(exec) => {
                    dirty.insert(tx.from);
                    if let Some(to) = tx.to {
                        dirty.insert(to);
                    }
                    if let Some(addr) = &exec.created_address {
                        dirty.insert(*addr);
                    }
                    dirty.insert(coinbase);
                    results.push((i, exec));
                }
                Err(_) => {
                    results.push((
                        i,
                        TxExecution {
                            success: false,
                            gas_used: 0,
                            output: Bytes::new(),
                            created_address: None,
                            logs: Vec::new(),
                        },
                    ));
                }
            }
        }

        ParallelExecutionResult {
            results,
            merged_db: fork_db,
            dirty_addresses: dirty,
        }
    }
}
