use anyhow::{Context, Result, anyhow};
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub mempool: MempoolConfig,
    #[serde(default)]
    pub mersennet_orders: MersennetOrdersConfig,
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
    #[serde(default)]
    pub ws: WsConfig,
    #[serde(default)]
    pub zk: ZkConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// `true` activates the shielded subsystems immediately at boot.
    /// In normal operation, leave `false` and let the engine flip
    /// the switch when `block_number >= activation_height`.
    #[serde(default)]
    pub mode_activated: bool,
    /// Block height at which privacy mode auto-activates. `None`
    /// disables auto-activation; governance must call
    /// `mersennet_activatePrivacy` explicitly.
    #[serde(default)]
    pub activation_height: Option<u64>,
    /// DKG epoch length in blocks.
    #[serde(default = "default_dkg_epoch_length")]
    pub dkg_epoch_length_blocks: u64,
    /// Threshold k-of-n for the encrypted mempool.
    #[serde(default = "default_threshold_k")]
    pub threshold_k: u32,
    /// Total validator count n for the encrypted mempool.
    #[serde(default = "default_threshold_n")]
    pub threshold_n: u32,
}

fn default_dkg_epoch_length() -> u64 {
    18_000
}

fn default_threshold_k() -> u32 {
    2
}

fn default_threshold_n() -> u32 {
    3
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            mode_activated: false,
            activation_height: None,
            dkg_epoch_length_blocks: default_dkg_epoch_length(),
            threshold_k: default_threshold_k(),
            threshold_n: default_threshold_n(),
        }
    }
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
    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,
}

fn default_storage_backend() -> String {
    "sled".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolConfig {
    #[serde(default = "default_mempool_max")]
    pub max_total: usize,
    #[serde(default = "default_mempool_per_sender")]
    pub max_per_sender: usize,
    #[serde(default = "default_mempool_bump")]
    pub bump_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MersennetOrdersConfig {
    #[serde(default = "default_mersennet_orders_initial_margin_bps")]
    pub initial_margin_bps: u64,
    #[serde(default = "default_mersennet_orders_maintenance_margin_bps")]
    pub maintenance_margin_bps: u64,
    /// Allow the unsigned, owner-spoofable state-mutating `mersennet_orders_*`
    /// RPC methods (testnet seeding convenience). MUST be false on mainnet.
    #[serde(default = "default_allow_unsigned_orders_rpc")]
    pub allow_unsigned_orders_rpc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "default_bridge_max_queue_len")]
    pub max_queue_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenesisConfig {
    #[serde(default)]
    pub accounts: Vec<GenesisAccount>,
    #[serde(default)]
    pub validators: Vec<GenesisValidator>,
    /// CLOB markets to create deterministically at genesis on every node.
    /// Seeding markets here (instead of via the unsigned `addMarket` RPC,
    /// which mutates one node's local state only) is what makes the order
    /// book consensus-deterministic under single-leader production.
    #[serde(default)]
    pub markets: Vec<GenesisMarket>,
    /// Collateral to credit at genesis (e.g. the market-maker owner) so
    /// it can quote immediately without an out-of-band deposit.
    #[serde(default)]
    pub collateral: Vec<GenesisCollateral>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAccount {
    pub address: String,
    pub balance: String,
    #[serde(default)]
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisValidator {
    pub address: String,
    pub stake: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisMarket {
    pub symbol: String,
    /// Price tick size (decimal string). Defaults to 1.
    #[serde(default = "default_market_tick")]
    pub tick_size: String,
    /// Lot size (decimal string). Defaults to 1.
    #[serde(default = "default_market_lot")]
    pub lot_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisCollateral {
    pub owner: String,
    pub amount: String,
}

fn default_market_tick() -> String {
    "1".to_string()
}
fn default_market_lot() -> String {
    "1".to_string()
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
    #[serde(default = "default_p2p_listen")]
    pub listen: String,
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(default = "default_block_time_ms")]
    pub block_time_ms: u64,
    // NOTE: the Noise transport module is not yet wired into the UDP gossip
    // layer, so this flag currently only affects a startup log line. Block,
    // transaction and vote authenticity are enforced at the application layer
    // (proposer signatures + signed txs + validator-verified votes), which
    // holds regardless of transport. Wiring Noise for confidentiality/DoS
    // resistance is tracked as a follow-up.
    #[serde(default)]
    pub noise_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsConfig {
    #[serde(default = "default_ws_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ws_addr")]
    pub addr: String,
}

fn default_ws_enabled() -> bool {
    false
}

fn default_ws_addr() -> String {
    "127.0.0.1:9945".to_string()
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            enabled: default_ws_enabled(),
            addr: default_ws_addr(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkConfig {
    #[serde(default = "default_zk_enabled")]
    pub enabled: bool,
    #[serde(default = "default_zk_checkpoint_interval")]
    pub checkpoint_interval: u64,
}

fn default_zk_enabled() -> bool {
    false
}

fn default_zk_checkpoint_interval() -> u64 {
    100
}

impl Default for ZkConfig {
    fn default() -> Self {
        Self {
            enabled: default_zk_enabled(),
            checkpoint_interval: default_zk_checkpoint_interval(),
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
            storage_backend: default_storage_backend(),
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

impl Default for MersennetOrdersConfig {
    fn default() -> Self {
        Self {
            initial_margin_bps: default_mersennet_orders_initial_margin_bps(),
            maintenance_margin_bps: default_mersennet_orders_maintenance_margin_bps(),
            allow_unsigned_orders_rpc: default_allow_unsigned_orders_rpc(),
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
            listen: default_p2p_listen(),
            peers: Vec::new(),
            block_time_ms: default_block_time_ms(),
            noise_enabled: false,
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

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn privacy_section_defaults_when_omitted() {
        let cfg: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(!cfg.privacy.mode_activated);
        assert!(cfg.privacy.activation_height.is_none());
        assert_eq!(cfg.privacy.threshold_k, 2);
        assert_eq!(cfg.privacy.threshold_n, 3);
    }

    #[test]
    fn privacy_testnet_5_of_7_parses() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "engine": { "chain_id": 7920, "storage_backend": "redb" },
                "privacy": {
                    "mode_activated": false,
                    "activation_height": 100,
                    "dkg_epoch_length_blocks": 1800,
                    "threshold_k": 5,
                    "threshold_n": 7
                }
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.engine.chain_id, 7920);
        assert_eq!(cfg.privacy.activation_height, Some(100));
        assert_eq!(cfg.privacy.dkg_epoch_length_blocks, 1800);
        assert_eq!(cfg.privacy.threshold_k, 5);
        assert_eq!(cfg.privacy.threshold_n, 7);
    }

    #[test]
    fn shipped_privacy_testnet_config_parses() {
        // Sanity-check that the bundled testnet config loads as
        // an AppConfig (catches schema drift between code +
        // operator-facing JSON).
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::PathBuf::from(manifest_dir)
            .join("..")
            .join("..")
            .join("testnet")
            .join("configs")
            .join("privacy")
            .join("validator-1.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let cfg: AppConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(cfg.engine.chain_id, 7920);
        assert_eq!(cfg.privacy.threshold_k, 5);
        assert_eq!(cfg.privacy.threshold_n, 7);
        assert!(cfg.privacy.activation_height.is_some());
    }
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

/// Mersennet testnet chain id — the Mersenne prime 2^17 − 1.
/// (Mainnet is 8191 = 2^13 − 1, see `crate::mainnet::MAINNET_CHAIN_ID`.)
pub const TESTNET_CHAIN_ID: u64 = 131_071;

fn default_chain_id() -> u64 {
    TESTNET_CHAIN_ID
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

fn default_mersennet_orders_initial_margin_bps() -> u64 {
    0
}

fn default_mersennet_orders_maintenance_margin_bps() -> u64 {
    0
}

fn default_allow_unsigned_orders_rpc() -> bool {
    // Defaults to false: the unsigned `mersennet_orders_*` mutation path lets a
    // caller act for an arbitrary `owner` with no signature (account/order
    // takeover on a public RPC). Orders now arrive as signed transactions to
    // the CLOB precompile (0x…0100), where the caller is the verified signer.
    // A private, firewalled seeding node may still opt in explicitly.
    false
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
    // Total supply cap: 2^89 - 1 wei (a Mersenne prime), ~618.97M MRSN at 18 decimals.
    (2u128.pow(89) - 1).to_string()
}

fn default_initial_reward() -> String {
    // Initial block reward: 2^61 - 1 wei (a Mersenne prime), ~2.3 MRSN at 18 decimals.
    (2u128.pow(61) - 1).to_string()
}

fn default_halving_interval() -> u64 {
    // 5th perfect number = 2^12 * (2^13 - 1); ~1.06 years per halving at 1s blocks.
    33_550_336
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

fn default_p2p_listen() -> String {
    "0.0.0.0:30303".to_string()
}

fn default_block_time_ms() -> u64 {
    1000
}
