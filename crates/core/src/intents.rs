//! Intent-based execution: users express trade intents, solvers compete to fill them optimally.

use revm::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Intent {
    pub id: B256,
    pub user: Address,
    pub kind: IntentKind,
    pub constraints: IntentConstraints,
    pub deadline: u64, // Block number deadline
    pub tip: U256,     // Tip for solver
    pub status: IntentStatus,
    pub created_at: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IntentKind {
    Swap {
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    },
    LimitOrder {
        market_id: u64,
        side: String, // "buy" or "sell"
        price: u64,
        amount: U256,
    },
    BatchSwap {
        steps: Vec<SwapStep>,
    },
    BridgeAndSwap {
        source_chain: u64,
        dest_chain: u64,
        token_in: Address,
        token_out: Address,
        amount: U256,
    },
    DCA {
        token_in: Address,
        token_out: Address,
        total_amount: U256,
        num_orders: u32,
        interval_blocks: u64,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwapStep {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub pool_id: Option<u64>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntentConstraints {
    pub max_slippage_bps: u64,
    pub max_gas: u64,
    pub partial_fill_allowed: bool,
    pub preferred_solvers: Vec<Address>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntentStatus {
    Pending,
    Solving,
    Solved,
    Executed,
    Expired,
    Cancelled,
    Failed,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Solution {
    pub id: B256,
    pub intent_id: B256,
    pub solver: Address,
    pub execution_plan: Vec<ExecutionStep>,
    pub expected_output: U256,
    pub gas_estimate: u64,
    pub score: u64, // Quality score (higher = better for user)
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionStep {
    Transfer {
        from: Address,
        to: Address,
        amount: U256,
    },
    Swap {
        pool: Address,
        token_in: Address,
        token_out: Address,
        amount: U256,
    },
    CLOBOrder {
        market_id: u64,
        side: String,
        price: u64,
        amount: U256,
    },
    Bridge {
        chain_id: u64,
        token: Address,
        amount: U256,
    },
}

#[allow(dead_code)]
pub struct SolverRegistration {
    pub address: Address,
    pub stake: U256,
    pub supported_intents: Vec<String>,
    pub reputation_score: u64,
    pub total_solved: u64,
}

#[allow(dead_code)]
pub struct IntentResult {
    pub intent_id: B256,
    pub solver: Address,
    pub output: U256,
    pub gas_used: u64,
    pub success: bool,
}

#[allow(dead_code)]
pub struct IntentStats {
    pub total_intents: u64,
    pub pending: usize,
    pub solved: u64,
    pub expired: u64,
    pub failed: u64,
    pub total_solver_count: usize,
    pub total_tip_earned: U256,
}

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("intent not found")]
    IntentNotFound,
    #[error("intent expired")]
    IntentExpired,
    #[error("unauthorized")]
    Unauthorized,
    #[error("solver not registered")]
    SolverNotRegistered,
    #[error("insufficient solver stake")]
    InsufficientStake,
    #[error("no solution found")]
    NoSolution,
    #[error("intent already solved")]
    AlreadySolved,
    #[error("duplicate intent")]
    Duplicate,
    #[error("invalid solution")]
    InvalidSolution,
}

#[allow(dead_code)]
pub struct IntentEngine {
    intents: HashMap<B256, Intent>,
    solutions: HashMap<B256, Vec<Solution>>,
    solvers: HashMap<Address, SolverRegistration>,
    auction_period_blocks: u64,
    min_solver_stake: U256,
    total_intents: u64,
    total_solved: u64,
    total_expired: u64,
    total_failed: u64,
    total_tip_earned: U256,
}

impl std::fmt::Debug for IntentEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntentEngine")
            .field("intents", &self.intents.len())
            .field("solvers", &self.solvers.len())
            .finish()
    }
}

impl IntentEngine {
    pub fn new(auction_period: u64, min_stake: U256) -> Self {
        Self {
            intents: HashMap::new(),
            solutions: HashMap::new(),
            solvers: HashMap::new(),
            auction_period_blocks: auction_period,
            min_solver_stake: min_stake,
            total_intents: 0,
            total_solved: 0,
            total_expired: 0,
            total_failed: 0,
            total_tip_earned: U256::ZERO,
        }
    }

    pub fn submit_intent(&mut self, intent: Intent) -> Result<B256, IntentError> {
        if self.intents.contains_key(&intent.id) {
            return Err(IntentError::Duplicate);
        }
        self.total_intents += 1;
        self.intents.insert(intent.id, intent.clone());
        Ok(intent.id)
    }

    pub fn cancel_intent(&mut self, intent_id: B256, user: Address) -> Result<(), IntentError> {
        let intent = self.intents.get(&intent_id).ok_or(IntentError::IntentNotFound)?;
        if intent.user != user {
            return Err(IntentError::Unauthorized);
        }
        if intent.status != IntentStatus::Pending && intent.status != IntentStatus::Solving {
            return Err(IntentError::AlreadySolved);
        }
        let intent = self.intents.get_mut(&intent_id).unwrap();
        intent.status = IntentStatus::Cancelled;
        Ok(())
    }

    pub fn register_solver(&mut self, registration: SolverRegistration) -> Result<(), IntentError> {
        if registration.stake < self.min_solver_stake {
            return Err(IntentError::InsufficientStake);
        }
        self.solvers.insert(registration.address, registration);
        Ok(())
    }

    pub fn submit_solution(&mut self, solution: Solution) -> Result<(), IntentError> {
        if !self.solvers.contains_key(&solution.solver) {
            return Err(IntentError::SolverNotRegistered);
        }
        let intent = self
            .intents
            .get(&solution.intent_id)
            .ok_or(IntentError::IntentNotFound)?;
        if intent.status == IntentStatus::Executed {
            return Err(IntentError::AlreadySolved);
        }
        if intent.status == IntentStatus::Expired || intent.status == IntentStatus::Cancelled {
            return Err(IntentError::IntentExpired);
        }
        let intent_id = solution.intent_id;
        self.solutions.entry(intent_id).or_default().push(solution);
        let intent = self.intents.get_mut(&intent_id).unwrap();
        if intent.status == IntentStatus::Pending {
            intent.status = IntentStatus::Solving;
        }
        Ok(())
    }

    pub fn select_best_solution(&self, intent_id: &B256) -> Option<&Solution> {
        self.solutions.get(intent_id)?.iter().max_by_key(|s| s.score)
    }

    pub fn execute_intent(
        &mut self,
        intent_id: B256,
        current_block: u64,
    ) -> Result<IntentResult, IntentError> {
        let intent = self.intents.get(&intent_id).ok_or(IntentError::IntentNotFound)?;
        if intent.deadline < current_block {
            return Err(IntentError::IntentExpired);
        }
        if intent.status != IntentStatus::Solved && intent.status != IntentStatus::Solving {
            return Err(IntentError::NoSolution);
        }

        let solution = self
            .select_best_solution(&intent_id)
            .cloned()
            .ok_or(IntentError::NoSolution)?;

        let success = true; // Simulated successful execution
        let output = solution.expected_output;
        let gas_used = solution.gas_estimate;

        if success {
            self.total_solved += 1;
            self.total_tip_earned += intent.tip;
            if let Some(solver) = self.solvers.get_mut(&solution.solver) {
                solver.total_solved += 1;
            }
        } else {
            self.total_failed += 1;
        }

        let intent = self.intents.get_mut(&intent_id).unwrap();
        intent.status = if success {
            IntentStatus::Executed
        } else {
            IntentStatus::Failed
        };

        Ok(IntentResult {
            intent_id,
            solver: solution.solver,
            output,
            gas_used,
            success,
        })
    }

    pub fn expire_intents(&mut self, current_block: u64) {
        for intent in self.intents.values_mut() {
            if intent.deadline < current_block
                && (intent.status == IntentStatus::Pending || intent.status == IntentStatus::Solving)
            {
                intent.status = IntentStatus::Expired;
                self.total_expired += 1;
            }
        }
    }

    pub fn pending_intents(&self) -> Vec<&Intent> {
        self.intents
            .values()
            .filter(|i| {
                i.status == IntentStatus::Pending || i.status == IntentStatus::Solving
            })
            .collect()
    }

    pub fn solver_stats(&self, solver: &Address) -> Option<&SolverRegistration> {
        self.solvers.get(solver)
    }

    pub fn stats(&self) -> IntentStats {
        let pending = self
            .intents
            .values()
            .filter(|i| {
                i.status == IntentStatus::Pending || i.status == IntentStatus::Solving
            })
            .count();
        IntentStats {
            total_intents: self.total_intents,
            pending,
            solved: self.total_solved,
            expired: self.total_expired,
            failed: self.total_failed,
            total_solver_count: self.solvers.len(),
            total_tip_earned: self.total_tip_earned,
        }
    }
}
