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
    /// Open (permissionless) validator set. Off unless `activation_height`
    /// is set; must be identical on every node (canonical config).
    #[serde(default)]
    pub validator_set: ValidatorSetConfig,
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
    #[serde(default)]
    pub watchdog: WatchdogConfig,
}

/// In-process liveness watchdog. A node whose head stops advancing while
/// the network moves on is wedged (a stuck thread, a poisoned lock, a dead
/// listener); exiting non-zero lets systemd (`Restart=always`) bring it back
/// in seconds. The first outside validator sat wedged for six hours before
/// its operator restarted it by hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Seconds the head may stay unchanged, while the network is known to be
    /// ahead, before the node exits for a restart.
    #[serde(default = "default_watchdog_stall_secs")]
    pub stall_secs: u64,
    /// How far ahead the network must be for "stalled" to count (blocks).
    #[serde(default = "default_watchdog_min_gap")]
    pub min_gap_blocks: u64,
    /// Seconds the engine lock may be unobtainable before the node exits.
    #[serde(default = "default_watchdog_lock_secs")]
    pub lock_secs: u64,
    /// Optional JSON-RPC URL used as a second opinion on the network head
    /// (gossip alone cannot report a height when the listener itself is what
    /// died). Empty = gossip only.
    #[serde(default)]
    pub reference_rpc: String,
}

fn default_true() -> bool {
    true
}
fn default_watchdog_stall_secs() -> u64 {
    300
}
fn default_watchdog_min_gap() -> u64 {
    60
}
fn default_watchdog_lock_secs() -> u64 {
    90
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stall_secs: default_watchdog_stall_secs(),
            min_gap_blocks: default_watchdog_min_gap(),
            lock_secs: default_watchdog_lock_secs(),
            reference_rpc: String::new(),
        }
    }
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
    /// Consensus switch for the fee floor and the fee split below. 0 = off:
    /// the base fee may fall to 1 wei and everything it collects is burned.
    #[serde(default)]
    pub fee_floor_height: u64,
    /// From `fee_floor_height` the base fee never falls below this many wei
    /// (the EIP-1559 adjustment still moves it up with usage). At 1 wei one
    /// account put 530k reverting transactions a day on the chain for free.
    #[serde(default)]
    pub min_base_fee_wei: u64,
    /// From `fee_floor_height`: share (bps) of the base fee collected by a
    /// block that is credited to `fee_treasury_address`.
    #[serde(default)]
    pub fee_treasury_bps: u64,
    /// From `fee_floor_height`: share (bps) of the base fee credited to the
    /// block's proposer (its reward recipient). What neither share takes is
    /// burned, as before.
    #[serde(default)]
    pub fee_proposer_bps: u64,
    /// Recipient of the treasury share (hex address). Required when
    /// `fee_treasury_bps` > 0.
    #[serde(default)]
    pub fee_treasury_address: Option<String>,
    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,
    /// Page-cache budget for the sled state store, in bytes (default 512 MiB;
    /// sled's own default is 1 GiB). Consensus-neutral. sled counts this
    /// budget approximately and the process settles at roughly 2–3× it plus
    /// the chain window, so a 4 GB validator wants 512 MiB or less; the
    /// default sled figure took the fleet to ~3 GB RSS and into the OOM
    /// killer (Sep 2026). More cache = fewer disk reads on RPC-heavy nodes.
    #[serde(default = "default_state_cache_bytes")]
    pub state_cache_bytes: u64,
    /// What to do when the restored state's Merkle root does not match the
    /// head block's state root at startup: `"warn"` (metric + error, keep
    /// running) or `"fatal"` (exit 5 so the operator resets state / the
    /// fleet heal restores a snapshot). Consensus-neutral; synced from the
    /// canonical config.
    #[serde(default = "default_resume_root_check")]
    pub resume_root_check: String,
}

fn default_storage_backend() -> String {
    "sled".to_string()
}
pub const MIN_STATE_CACHE_BYTES: u64 = 64 * 1024 * 1024;
fn default_state_cache_bytes() -> u64 {
    512 * 1024 * 1024
}
fn default_resume_root_check() -> String {
    "warn".to_string()
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
    /// Consensus switch: height from which agent delegation is accepted on the
    /// CLOB precompile (`setAgent`/`revokeAgent`, orders and cancels signed by
    /// an agent key act for the granting account). 0 = off.
    #[serde(default)]
    pub agent_delegation_height: u64,
    /// Consensus switch: at this height every market listed in
    /// `price_rescales` is rescaled in place (orders, positions and last price
    /// multiplied; notional/PnL divided by the scale from then on). 0 = off.
    #[serde(default)]
    pub price_scale_height: u64,
    /// `[market_id, new_scale]` pairs applied at `price_scale_height`.
    #[serde(default)]
    pub price_rescales: Vec<(u64, u64)>,
    /// Consensus switch: from this height the CLOB and staking precompiles
    /// authorise on the immediate caller of the call frame (`msg.sender`)
    /// instead of the transaction origin, so contracts own their own accounts
    /// and a contract cannot act on the account of the user calling it. 0 = off.
    #[serde(default)]
    pub frame_caller_height: u64,
    /// Consensus switch: from this height a business error in the CLOB or
    /// staking precompile (insufficient collateral, unknown market, not the
    /// order's owner, …) is a proper EVM revert — `Error(string)` output the
    /// caller can read, and only the call's base gas is charged — instead of
    /// a precompile halt that returned nothing and burned the whole gas limit
    /// (300k per refused order; 530k of them a day on 20–25 Sep). 0 = off.
    #[serde(default)]
    pub revert_reasons_height: u64,
    /// Consensus switch: from this height one collateral unit is one MRSN
    /// (deposits escrow amount × 1e18 wei), realized PnL settles into
    /// collateral at every fill, and the margin parameters below replace
    /// `initial_margin_bps` / `maintenance_margin_bps`. Pre-switch balances
    /// (wei-backed units) are divided by 1e18 once at the switch. 0 = off.
    #[serde(default)]
    pub settlement_height: u64,
    #[serde(default)]
    pub settlement_initial_margin_bps: u64,
    #[serde(default)]
    pub settlement_maintenance_margin_bps: u64,
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
    /// Non-native tokens accepted as margin collateral, registered at
    /// genesis on every node so the collateral registry is deterministic.
    #[serde(default)]
    pub collateral_assets: Vec<GenesisCollateralAsset>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisCollateralAsset {
    /// ERC-20 token address.
    pub token: String,
    /// Collateral haircut in basis points (e.g. 9000 = 90% of value counts).
    pub weight_bps: u64,
    /// Margin value per raw token unit = value_num / value_den, in CLOB
    /// collateral units (the same integer units `depositCollateral` escrows).
    /// For a 6-decimal USD stable where 1 token should count as 1 collateral
    /// unit, use value_num = 1, value_den = 10^6.
    pub value_num: String,
    pub value_den: String,
    /// Storage slot index of the token's `balanceOf` mapping (MockERC20 = 3).
    #[serde(default = "default_balances_slot")]
    pub balances_slot: u64,
}

fn default_balances_slot() -> u64 {
    3
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorSetConfig {
    /// 0 = disabled (genesis set only).
    #[serde(default)]
    pub activation_height: u64,
    #[serde(default = "default_epoch_blocks")]
    pub epoch_blocks: u64,
    /// In whole MRSN (converted to wei internally).
    #[serde(default = "default_min_self_stake_mrsn")]
    pub min_self_stake_mrsn: u64,
    #[serde(default = "default_max_validators")]
    pub max_validators: usize,
    #[serde(default = "default_vs_unbonding_blocks")]
    pub unbonding_blocks: u64,
    #[serde(default = "default_jail_miss_bps")]
    pub jail_miss_bps: u64,
    #[serde(default = "default_jail_min_slots")]
    pub jail_min_slots: u64,
    /// Height from which block rewards go to the operator wallet (0 = off).
    #[serde(default)]
    pub rewards_to_operator_height: u64,
    /// Height from which repeat jails escalate 1, 2, 4, 8, 16, 24 epochs (0 = off).
    #[serde(default)]
    pub jail_escalation_height: u64,
    /// Height from which a leader that missed 3 slots is benched for the rest of the epoch (0 = off).
    #[serde(default)]
    pub bench_height: u64,
    /// Height from which the active set holds up to `max_validators_after` (0 = off).
    #[serde(default)]
    pub max_validators_height: u64,
    #[serde(default)]
    pub max_validators_after: usize,
}

fn default_epoch_blocks() -> u64 {
    1_800
}
fn default_min_self_stake_mrsn() -> u64 {
    1_000
}
fn default_max_validators() -> usize {
    12
}
fn default_vs_unbonding_blocks() -> u64 {
    7_200
}
fn default_jail_miss_bps() -> u64 {
    2_000
}
fn default_jail_min_slots() -> u64 {
    5
}

impl ValidatorSetConfig {
    pub fn to_params(&self) -> crate::staking::ValidatorSetParams {
        let d = crate::staking::ValidatorSetParams::default();
        crate::staking::ValidatorSetParams {
            activation_height: self.activation_height,
            epoch_blocks: if self.epoch_blocks == 0 {
                d.epoch_blocks
            } else {
                self.epoch_blocks
            },
            min_self_stake: revm::primitives::U256::from(self.min_self_stake_mrsn)
                * revm::primitives::U256::from(10u64).pow(revm::primitives::U256::from(18u64)),
            max_validators: if self.max_validators == 0 {
                d.max_validators
            } else {
                self.max_validators
            },
            unbonding_blocks: if self.unbonding_blocks == 0 {
                d.unbonding_blocks
            } else {
                self.unbonding_blocks
            },
            jail_miss_bps: self.jail_miss_bps,
            jail_min_slots: self.jail_min_slots,
            rewards_to_operator_height: self.rewards_to_operator_height,
            jail_escalation_height: self.jail_escalation_height,
            bench_height: self.bench_height,
            max_validators_height: self.max_validators_height,
            max_validators_after: self.max_validators_after,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rpc_addr")]
    pub addr: String,
    /// Client IPs exempt from the per-IP request limiter (e.g. this
    /// operator's own indexer/bots host). Everyone else stays limited.
    #[serde(default)]
    pub trusted_ips: Vec<String>,
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
    /// Wallet address of whoever operates this node. Included in the node's
    /// signed `whoami` attestation so the operator can claim the node on
    /// trade.mersennet.com (verified node runner). Optional.
    #[serde(default)]
    pub operator_address: Option<String>,
    #[serde(default = "default_block_time_ms")]
    pub block_time_ms: u64,
    /// Height from which this node encodes gossip payloads as base64 instead
    /// of JSON byte arrays (about 2.8x fewer bytes on the wire). Every build
    /// since 2026-09-15 decodes both, so the switch is gated by height to let
    /// a fleet roll before anyone emits the compact form. 0 = from the start.
    #[serde(default)]
    pub compact_wire_height: u64,
    /// Height from which the leader failover round is 3 block-times + 2 s
    /// (8 s at 2 s blocks) instead of the original 8 block-times + 3 s
    /// (19 s). Sized when large blocks could not propagate; with TCP block
    /// push a dead leader should cost the chain seconds, not a third of a
    /// minute. Gated by height so every node rotates on the same clock.
    /// 0 = from the start.
    #[serde(default)]
    pub fast_failover_height: u64,
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
            state_cache_bytes: default_state_cache_bytes(),
            gas_limit_per_block: default_gas_limit(),
            fee_elasticity_multiplier: default_fee_elasticity(),
            fee_max_change_denominator: default_fee_change_denominator(),
            fee_floor_height: 0,
            min_base_fee_wei: 0,
            fee_treasury_bps: 0,
            fee_proposer_bps: 0,
            fee_treasury_address: None,
            storage_backend: default_storage_backend(),
            resume_root_check: default_resume_root_check(),
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
            agent_delegation_height: 0,
            price_scale_height: 0,
            price_rescales: Vec::new(),
            frame_caller_height: 0,
            revert_reasons_height: 0,
            settlement_height: 0,
            settlement_initial_margin_bps: 0,
            settlement_maintenance_margin_bps: 0,
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
            trusted_ips: Vec::new(),
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
            compact_wire_height: 0,
            fast_failover_height: 0,
            noise_enabled: false,
            operator_address: None,
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
        assert_eq!(cfg.engine.chain_id, 524287);
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

#[cfg(test)]
mod canonical_config_tests {
    use super::*;

    /// The canonical testnet config must parse into `AppConfig` and carry
    /// coherent consensus switches: every rescale names a positive scale and
    /// the switch heights are epoch-aligned when the open set is active.
    #[test]
    fn canonical_testnet_config_parses_and_switches_are_coherent() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../networks/testnet/config.json"
        );
        let cfg = load_config(path).expect("canonical config parses");
        assert_eq!(cfg.engine.chain_id, 131071);
        let o = &cfg.mersennet_orders;
        if o.price_scale_height > 0 {
            assert!(
                !o.price_rescales.is_empty(),
                "a price switch must list markets"
            );
            for (m, sc) in &o.price_rescales {
                assert!(*m > 0 && *sc > 1, "rescale {m} -> {sc}");
            }
        }
        let vs = &cfg.validator_set;
        for (name, h) in [
            ("agent_delegation_height", o.agent_delegation_height),
            ("price_scale_height", o.price_scale_height),
            ("frame_caller_height", o.frame_caller_height),
            ("revert_reasons_height", o.revert_reasons_height),
            ("fee_floor_height", cfg.engine.fee_floor_height),
            ("settlement_height", o.settlement_height),
            ("bench_height", vs.bench_height),
            ("jail_escalation_height", vs.jail_escalation_height),
        ] {
            if h > 0 && vs.epoch_blocks > 0 {
                assert_eq!(
                    h % vs.epoch_blocks,
                    0,
                    "{name} should sit on an epoch boundary"
                );
            }
        }
        let e = &cfg.engine;
        assert!(
            e.state_cache_bytes >= MIN_STATE_CACHE_BYTES,
            "engine.state_cache_bytes must be at least 64 MiB"
        );
        assert!(
            e.fee_treasury_bps + e.fee_proposer_bps <= 10_000,
            "fee_treasury_bps + fee_proposer_bps must not exceed 10000"
        );
        if e.fee_treasury_bps > 0 {
            let addr = e.fee_treasury_address.as_deref().unwrap_or("");
            assert!(
                addr.len() == 42
                    && addr.starts_with("0x")
                    && addr[2..].chars().all(|c| c.is_ascii_hexdigit()),
                "fee_treasury_address must be a 0x-prefixed 20-byte hex address when fee_treasury_bps > 0"
            );
        }
    }
}
