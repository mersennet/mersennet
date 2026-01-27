use anyhow::{anyhow, Context, Result};
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub mempool: MempoolConfig,
    #[serde(default)]
    pub prime_orders: PrimeOrdersConfig,
    #[serde(default)]
    pub bridge: BridgeConfig,
    #[serde(default)]
    pub genesis: GenesisConfig,
    #[serde(default)]
    pub slashing: SlashingConfig,
    #[serde(default)]
    pub token_economics: TokenEconomicsConfig,
    #[serde(default)]
    pub rpc: RpcConfig,
    #[serde(default)]
    pub p2p: P2pConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default = "default_chain_id")]
    pub chain_id: u64,
    #[serde(default = "default_state_path")]
    pub state_path: String,
    #[serde(default = "default_gas_limit")]
    pub gas_limit_per_block: u64,
    #[serde(default = "default_fee_elasticity")]
    pub fee_elasticity_multiplier: u64,
    #[serde(default = "default_fee_change_denominator")]
    pub fee_max_change_denominator: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolConfig {
    #[serde(default = "default_mempool_max")]
    pub max_total: usize,
    #[serde(default = "default_mempool_per_sender")]
    pub max_per_sender: usize,
    #[serde(default = "default_mempool_bump")]
    pub bump_bps: u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeOrdersConfig {
    #[serde(default = "default_prime_orders_initial_margin_bps")]
    pub initial_margin_bps: u64,
    #[serde(default = "default_prime_orders_maintenance_margin_bps")]
    pub maintenance_margin_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "default_bridge_max_queue_len")]
    pub max_queue_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    #[serde(default)]
    pub accounts: Vec<GenesisAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAccount {
    pub address: String,
    pub balance: String,
    #[serde(default)]
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashingConfig {
    #[serde(default = "default_double_sign_bps")]
    pub double_sign_bps: u64,
    #[serde(default = "default_timeout_bps")]
    pub timeout_bps: u64,
    #[serde(default = "default_escalation_step_bps")]
    pub escalation_step_bps: u64,
    #[serde(default = "default_escalation_max_bps")]
    pub escalation_max_bps: u64,
    #[serde(default = "default_round_timeout_ms")]
    pub round_timeout_ms: u64,
    #[serde(default = "default_unbonding_period")]
    pub unbonding_period: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEconomicsConfig {
    #[serde(default = "default_max_supply")]
    pub max_supply: String,
    #[serde(default = "default_initial_reward")]
    pub initial_reward_per_block: String,
    #[serde(default = "default_halving_interval")]
    pub halving_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rpc_addr")]
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    #[serde(default = "default_node_key_path")]
    pub node_key_path: String,
    #[serde(default = "default_peer_store_path")]
    pub peer_store_path: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            mempool: MempoolConfig::default(),
            prime_orders: PrimeOrdersConfig::default(),
            bridge: BridgeConfig::default(),
            genesis: GenesisConfig::default(),
            slashing: SlashingConfig::default(),
            token_economics: TokenEconomicsConfig::default(),
            rpc: RpcConfig::default(),
            p2p: P2pConfig::default(),
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            chain_id: default_chain_id(),
            state_path: default_state_path(),
            gas_limit_per_block: default_gas_limit(),
            fee_elasticity_multiplier: default_fee_elasticity(),
            fee_max_change_denominator: default_fee_change_denominator(),
        }
    }
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_total: default_mempool_max(),
            max_per_sender: default_mempool_per_sender(),
            bump_bps: default_mempool_bump(),
        }
    }
}

impl Default for PrimeOrdersConfig {
    fn default() -> Self {
        Self {
            initial_margin_bps: default_prime_orders_initial_margin_bps(),
            maintenance_margin_bps: default_prime_orders_maintenance_margin_bps(),
        }
    }
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            max_queue_len: default_bridge_max_queue_len(),
        }
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self { accounts: Vec::new() }
    }
}

impl Default for SlashingConfig {
    fn default() -> Self {
        Self {
            double_sign_bps: default_double_sign_bps(),
            timeout_bps: default_timeout_bps(),
            escalation_step_bps: default_escalation_step_bps(),
            escalation_max_bps: default_escalation_max_bps(),
            round_timeout_ms: default_round_timeout_ms(),
            unbonding_period: default_unbonding_period(),
        }
    }
}

impl Default for TokenEconomicsConfig {
    fn default() -> Self {
        Self {
            max_supply: default_max_supply(),
            initial_reward_per_block: default_initial_reward(),
            halving_interval: default_halving_interval(),
        }
    }
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: default_rpc_addr(),
        }
    }
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            node_key_path: default_node_key_path(),
            peer_store_path: default_peer_store_path(),
        }
    }
}

pub fn load_config(path: &str) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {path}"))?;
    let config: AppConfig = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse config file {path}"))?;
    Ok(config)
}

pub fn parse_u256(value: &str) -> Result<U256> {
    if let Some(stripped) = value.strip_prefix("0x") {
        U256::from_str_radix(stripped, 16).context("invalid hex U256")
    } else {
        U256::from_str_radix(value, 10).context("invalid decimal U256")
    }
}

pub fn parse_address(value: &str) -> Result<Address> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(stripped).context("invalid address hex")?;
    if bytes.len() != 20 {
        return Err(anyhow!("invalid address length"));
    }
    Ok(Address::from_slice(&bytes))
}

fn default_chain_id() -> u64 {
    999
}

fn default_state_path() -> String {
    "state".to_string()
}

fn default_gas_limit() -> u64 {
    30_000_000
}

fn default_fee_elasticity() -> u64 {
    2
}

fn default_fee_change_denominator() -> u64 {
    8
}

fn default_mempool_max() -> usize {
    10_000
}

fn default_mempool_per_sender() -> usize {
    1_000
}

fn default_mempool_bump() -> u64 {
    1_000
}

fn default_prime_orders_initial_margin_bps() -> u64 {
    0
}

fn default_prime_orders_maintenance_margin_bps() -> u64 {
    0
}

fn default_bridge_max_queue_len() -> usize {
    10_000
}

fn default_double_sign_bps() -> u64 {
    500
}

fn default_timeout_bps() -> u64 {
    100
}

fn default_escalation_step_bps() -> u64 {
    25
}

fn default_escalation_max_bps() -> u64 {
    1_000
}

fn default_round_timeout_ms() -> u64 {
    500
}

fn default_unbonding_period() -> u64 {
    2
}

fn default_max_supply() -> String {
    let decimals = 1_000_000_000_000_000_000u128;
    let max = 42_000_000u128 * decimals;
    max.to_string()
}

fn default_initial_reward() -> String {
    let decimals = 1_000_000_000_000_000_000u128;
    let reward = 100u128 * decimals;
    reward.to_string()
}

fn default_halving_interval() -> u64 {
    4_200_000
}

fn default_rpc_addr() -> String {
    "127.0.0.1:8545".to_string()
}

fn default_node_key_path() -> String {
    "state/node_key.json".to_string()
}

fn default_peer_store_path() -> String {
    "state/peers.json".to_string()
}
