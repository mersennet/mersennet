//! Block production pipeline for Prime Chain.
//!
//! Overlaps execution of block N+1 with consensus of block N to effectively double throughput.

use crate::engine::{Receipt, Transaction};
use revm::primitives::{Address, B256};
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PipelineConfig {
    pub block_time_ms: u64,
    pub max_txs_per_block: usize,
    pub pipeline_depth: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            block_time_ms: 1000,
            max_txs_per_block: 1000,
            pipeline_depth: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// State diff for tracking execution changes
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct StateDiff {
    pub dirty_accounts: Vec<Address>,
    pub new_state_root: B256,
}

// ---------------------------------------------------------------------------
// Pre-executed block
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ExecutedBlock {
    pub height: u64,
    pub transactions: Vec<Transaction>,
    pub receipts: Vec<Receipt>,
    pub gas_used: u64,
    pub execution_time: Duration,
    pub state_diff: StateDiff,
}

// ---------------------------------------------------------------------------
// Pipeline statistics
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct PipelineStats {
    pub blocks_produced: u64,
    pub avg_execution_ms: f64,
    pub avg_consensus_ms: f64,
    pub pipeline_utilization: f64,
    pub total_txs_processed: u64,
}

impl PipelineStats {
    fn record_execution(&mut self, duration: Duration, tx_count: u64) {
        let ms = duration.as_secs_f64() * 1000.0;
        if self.blocks_produced == 0 {
            self.avg_execution_ms = ms;
        } else {
            self.avg_execution_ms = (self.avg_execution_ms * self.blocks_produced as f64 + ms)
                / (self.blocks_produced + 1) as f64;
        }
        self.total_txs_processed += tx_count;
    }

    fn record_consensus(&mut self, duration: Duration) {
        let ms = duration.as_secs_f64() * 1000.0;
        if self.blocks_produced == 0 {
            self.avg_consensus_ms = ms;
        } else {
            self.avg_consensus_ms = (self.avg_consensus_ms * self.blocks_produced as f64 + ms)
                / (self.blocks_produced + 1) as f64;
        }
        self.blocks_produced += 1;
    }

    fn update_utilization(&mut self, execution_ms: f64, consensus_ms: f64, block_time_ms: f64) {
        if block_time_ms <= 0.0 {
            self.pipeline_utilization = 0.0;
            return;
        }
        let overlap = (execution_ms + consensus_ms - block_time_ms).max(0.0);
        let max_possible = execution_ms.max(consensus_ms) * 2.0;
        self.pipeline_utilization = if max_possible > 0.0 {
            (overlap / block_time_ms * 100.0).min(100.0)
        } else {
            0.0
        };
    }
}

// ---------------------------------------------------------------------------
// Block pipeline
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct BlockPipeline {
    config: PipelineConfig,
    pending_executions: VecDeque<ExecutedBlock>,
    stats: PipelineStats,
    execution_durations: Vec<Duration>,
    consensus_durations: Vec<Duration>,
}

impl BlockPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            config,
            pending_executions: VecDeque::new(),
            stats: PipelineStats::default(),
            execution_durations: Vec::new(),
            consensus_durations: Vec::new(),
        }
    }

    pub fn push_executed(&mut self, block: ExecutedBlock) {
        self.execution_durations.push(block.execution_time);
        self.pending_executions.push_back(block);
    }

    pub fn pop_for_consensus(&mut self) -> Option<ExecutedBlock> {
        let block = self.pending_executions.pop_front();
        if let Some(ref b) = block {
            self.stats
                .record_execution(b.execution_time, b.transactions.len() as u64);
        }
        block
    }

    pub fn is_full(&self) -> bool {
        self.pending_executions.len() >= self.config.pipeline_depth
    }

    pub fn stats(&self) -> &PipelineStats {
        &self.stats
    }

    pub fn record_consensus_complete(&mut self, duration: Duration) {
        self.consensus_durations.push(duration);
        self.stats.record_consensus(duration);

        if let (Some(&exec), Some(&cons)) = (
            self.execution_durations.first(),
            self.consensus_durations.last(),
        ) {
            self.stats.update_utilization(
                exec.as_secs_f64() * 1000.0,
                cons.as_secs_f64() * 1000.0,
                self.config.block_time_ms as f64,
            );
        }
        if !self.execution_durations.is_empty() {
            self.execution_durations.remove(0);
        }
    }

    pub fn record_execution_complete(&mut self, duration: Duration) {
        self.execution_durations.push(duration);
    }
}

// ---------------------------------------------------------------------------
// Orchestrator commands and results
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum PipelineCommand {
    ExecuteNext,
    Shutdown,
}

#[allow(dead_code)]
pub struct ConsensusResult {
    pub height: u64,
    pub success: bool,
    pub duration: Duration,
}

// ---------------------------------------------------------------------------
// Pipeline orchestrator
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct PipelineOrchestrator {
    execution_tx: Sender<PipelineCommand>,
    consensus_rx: Receiver<ConsensusResult>,
    stats: Arc<Mutex<PipelineStats>>,
}

impl PipelineOrchestrator {
    pub fn start<F, G>(execution_fn: F, consensus_fn: G) -> Self
    where
        F: Fn() -> Option<ExecutedBlock> + Send + 'static,
        G: Fn(ExecutedBlock) -> ConsensusResult + Send + 'static,
    {
        let (execution_tx, execution_rx) = std::sync::mpsc::channel();
        let (executed_tx, executed_rx) = std::sync::mpsc::channel();
        let (consensus_tx, consensus_rx) = std::sync::mpsc::channel();
        let stats = Arc::new(Mutex::new(PipelineStats::default()));

        // Execution thread: drains via execution_fn, sends ExecutedBlock to consensus
        std::thread::spawn(move || {
            loop {
                match execution_rx.try_recv() {
                    Ok(PipelineCommand::Shutdown) => break,
                    Ok(PipelineCommand::ExecuteNext) | Err(_) => {}
                }

                if let Some(block) = execution_fn() {
                    if executed_tx.send(block).is_err() {
                        break;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });

        let stats_cons = Arc::clone(&stats);

        // Consensus thread: receives ExecutedBlock, runs consensus, sends result
        std::thread::spawn(move || {
            while let Ok(block) = executed_rx.recv() {
                if let Ok(mut s) = stats_cons.lock() {
                    s.record_execution(block.execution_time, block.transactions.len() as u64);
                }
                let result = consensus_fn(block);
                if let Ok(mut s) = stats_cons.lock() {
                    s.record_consensus(result.duration);
                }
                if consensus_tx.send(result).is_err() {
                    break;
                }
            }
        });

        Self {
            execution_tx,
            consensus_rx,
            stats,
        }
    }

    pub fn send_execute(&self) {
        let _ = self.execution_tx.send(PipelineCommand::ExecuteNext);
    }

    pub fn send_shutdown(&self) {
        let _ = self.execution_tx.send(PipelineCommand::Shutdown);
    }

    pub fn try_recv_consensus(&self) -> Option<ConsensusResult> {
        self.consensus_rx.try_recv().ok()
    }

    pub fn recv_consensus(&self) -> Result<ConsensusResult, std::sync::mpsc::RecvError> {
        self.consensus_rx.recv()
    }

    pub fn stats(&self) -> std::sync::MutexGuard<'_, PipelineStats> {
        self.stats.lock().unwrap()
    }
}
