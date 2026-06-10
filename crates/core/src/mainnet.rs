//! Mainnet runtime checks and safety mechanisms for production.
//! Provides emergency halt, genesis validation, and block-level guards.

use revm::primitives::{Address, U256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Mainnet chain ID.
pub const MAINNET_CHAIN_ID: u64 = 8191;

/// Minimum number of validators required at genesis.
pub const MIN_GENESIS_VALIDATORS: usize = 7;

#[derive(Debug, thiserror::Error)]
pub enum MainnetError {
    #[error("chain is halted: {0}")]
    ChainHalted(String),
    #[error("unauthorized caller")]
    Unauthorized,
    #[error("invalid genesis: {0}")]
    InvalidGenesis(String),
    #[error("block time violation")]
    BlockTimeViolation,
    #[error("gas limit exceeded")]
    GasLimitExceeded,
}

/// Parameters for genesis validation.
#[allow(dead_code)]
pub struct GenesisValidation {
    pub chain_id: u64,
    pub validator_count: usize,
    pub total_supply: U256,
    pub genesis_timestamp: u64,
}

/// Runtime guard for mainnet safety checks.
#[allow(dead_code)]
pub struct MainnetGuard {
    chain_id: u64,
    is_mainnet: bool,
    launch_timestamp: u64,
    emergency_contacts: Vec<Address>,
    halted: bool,
    halt_reason: Option<String>,
}

impl std::fmt::Debug for MainnetGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainnetGuard")
            .field("chain_id", &self.chain_id)
            .field("is_mainnet", &self.is_mainnet)
            .field("halted", &self.halted)
            .finish()
    }
}

impl MainnetGuard {
    /// Create a new mainnet guard for the given chain ID.
    pub fn new(chain_id: u64) -> Self {
        let is_mainnet = chain_id == MAINNET_CHAIN_ID;
        let launch_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            chain_id,
            is_mainnet,
            launch_timestamp,
            emergency_contacts: Vec::new(),
            halted: false,
            halt_reason: None,
        }
    }

    /// Returns true if chain_id == 8191 (mainnet).
    pub fn is_mainnet(&self) -> bool {
        self.is_mainnet
    }

    /// Run checks before processing a block.
    pub fn pre_block_checks(&self, _height: u64) -> Result<(), MainnetError> {
        if self.halted {
            return Err(MainnetError::ChainHalted(
                self.halt_reason
                    .clone()
                    .unwrap_or_else(|| "emergency halt".to_string()),
            ));
        }
        Ok(())
    }

    /// Run checks after processing a block.
    pub fn post_block_checks(
        &self,
        _height: u64,
        _gas_used: u64,
        _tx_count: usize,
    ) -> Result<(), MainnetError> {
        if self.halted {
            return Err(MainnetError::ChainHalted(
                self.halt_reason
                    .clone()
                    .unwrap_or_else(|| "emergency halt".to_string()),
            ));
        }
        Ok(())
    }

    /// Emergency halt. Only emergency contacts can halt.
    pub fn emergency_halt(&mut self, reason: String, caller: Address) -> Result<(), MainnetError> {
        if !self.emergency_contacts.contains(&caller) {
            return Err(MainnetError::Unauthorized);
        }
        self.halted = true;
        self.halt_reason = Some(reason);
        Ok(())
    }

    /// Resume after halt. Only emergency contacts can resume.
    pub fn resume(&mut self, caller: Address) -> Result<(), MainnetError> {
        if !self.emergency_contacts.contains(&caller) {
            return Err(MainnetError::Unauthorized);
        }
        self.halted = false;
        self.halt_reason = None;
        Ok(())
    }

    /// Returns true if the chain is halted.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Add an emergency contact address.
    pub fn add_emergency_contact(&mut self, address: Address) {
        if !self.emergency_contacts.contains(&address) {
            self.emergency_contacts.push(address);
        }
    }

    /// Validate genesis parameters.
    pub fn validate_genesis(&self, genesis: &GenesisValidation) -> Result<(), MainnetError> {
        if genesis.chain_id != self.chain_id {
            return Err(MainnetError::InvalidGenesis(format!(
                "chain_id mismatch: expected {}, got {}",
                self.chain_id, genesis.chain_id
            )));
        }
        if genesis.validator_count < MIN_GENESIS_VALIDATORS {
            return Err(MainnetError::InvalidGenesis(format!(
                "validator count {} below minimum {}",
                genesis.validator_count, MIN_GENESIS_VALIDATORS
            )));
        }
        if genesis.total_supply.is_zero() {
            return Err(MainnetError::InvalidGenesis(
                "total_supply must be non-zero".to_string(),
            ));
        }
        Ok(())
    }
}
