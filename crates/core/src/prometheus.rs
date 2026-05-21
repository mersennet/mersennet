use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;

static PROM_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

/// Initialize the Prometheus exporter, register all metric descriptions, and
/// return a static handle.  Safe to call multiple times; first call installs
/// the recorder.
pub fn init() -> &'static PrometheusHandle {
    PROM_HANDLE.get_or_init(|| {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .expect("failed to install Prometheus metrics exporter");
        MetricsRegistry::register_all();
        handle
    })
}

/// Get the handle if already initialized.
pub fn handle() -> Option<&'static PrometheusHandle> {
    PROM_HANDLE.get()
}

/// Central registry that pre-describes every Prometheus metric used by Prime
/// Chain.  Calling [`register_all`] is idempotent and emits `describe_*` calls
/// so that the `/metrics` endpoint always includes HELP / TYPE lines even
/// before the first sample is recorded.
pub struct MetricsRegistry;

impl MetricsRegistry {
    pub fn register_all() {
        // -- Node --
        metrics::describe_gauge!(
            "prime_chain_up",
            "Node liveness indicator (always 1 while running)"
        );

        // -- Engine / block production --
        metrics::describe_counter!(
            "prime_chain_blocks_produced_total",
            "Total number of blocks produced"
        );
        metrics::describe_gauge!(
            "prime_chain_block_gas_used",
            "Gas consumed by the latest block"
        );
        metrics::describe_gauge!(
            "prime_chain_block_tx_count",
            "Transaction count in the latest block"
        );
        metrics::describe_histogram!(
            "prime_chain_block_execution_seconds",
            "Block execution wall-clock duration in seconds"
        );
        metrics::describe_gauge!(
            "prime_chain_base_fee_wei",
            "Current EIP-1559 base fee in wei"
        );
        metrics::describe_gauge!(
            "prime_chain_height",
            "Latest committed block height"
        );

        // -- Consensus --
        metrics::describe_counter!(
            "prime_chain_consensus_rounds",
            "Total consensus rounds executed"
        );
        metrics::describe_counter!(
            "prime_chain_consensus_finalized",
            "Total blocks finalized via consensus"
        );
        metrics::describe_counter!(
            "prime_chain_slashing_events",
            "Slashing evidence events by kind"
        );
        metrics::describe_gauge!(
            "prime_chain_validators_active",
            "Number of active (non-jailed, non-tombstoned) validators"
        );
        metrics::describe_gauge!(
            "prime_chain_total_stake",
            "Total staked amount across all validators"
        );

        // -- PrimeOrders --
        metrics::describe_counter!(
            "prime_chain_orders_submitted",
            "Total orders submitted to the order book"
        );
        metrics::describe_counter!(
            "prime_chain_orders_filled",
            "Total orders fully filled"
        );
        metrics::describe_counter!(
            "prime_chain_orders_cancelled",
            "Total orders cancelled"
        );
        metrics::describe_counter!(
            "prime_chain_trades_executed",
            "Total trade fills executed"
        );
        metrics::describe_gauge!(
            "prime_chain_insurance_fund_balance",
            "Insurance fund balance"
        );
        metrics::describe_gauge!(
            "prime_chain_markets_active",
            "Number of active markets"
        );

        // -- Mempool --
        metrics::describe_gauge!(
            "prime_chain_mempool_size",
            "Current number of transactions in the mempool"
        );
        metrics::describe_counter!(
            "prime_chain_mempool_rejected",
            "Transactions rejected by the mempool by reason"
        );

        // -- RPC --
        metrics::describe_counter!(
            "prime_chain_rpc_requests",
            "Total RPC requests by method"
        );
        metrics::describe_counter!(
            "prime_chain_rpc_errors",
            "Total RPC errors by method and code"
        );
        metrics::describe_histogram!(
            "prime_chain_rpc_duration_seconds",
            "RPC request duration in seconds by method"
        );

        // -- Privacy (Workstream H7) ------------------------------
        //
        // CI invariant K2 forbids address-labeled metrics. Every
        // metric here is either a chain-global aggregate or labeled
        // only by market_id / proof_kind / epoch — never by EOA.
        metrics::describe_gauge!(
            "prime_chain_privacy_mode_active",
            "1 iff privacy hard fork is active, 0 otherwise"
        );
        metrics::describe_counter!(
            "prime_chain_shielded_root_advanced_total",
            "Total times the shielded note tree root advanced"
        );
        metrics::describe_gauge!(
            "prime_chain_shielded_notes_total",
            "Number of notes in the shielded tree"
        );
        metrics::describe_gauge!(
            "prime_chain_shielded_nullifiers_total",
            "Number of spent nullifiers (chain-wide aggregate)"
        );
        metrics::describe_counter!(
            "prime_chain_threshold_mempool_admitted_total",
            "Total threshold-encrypted intents admitted"
        );
        metrics::describe_gauge!(
            "prime_chain_threshold_mempool_pending",
            "Number of intents pending threshold decryption"
        );
        metrics::describe_histogram!(
            "prime_chain_threshold_decryption_seconds",
            "Latency to recover plaintext via Lagrange interpolation"
        );
        metrics::describe_counter!(
            "prime_chain_fba_cleared_total",
            "Total FBA tick clearings (labeled by market_id, no addresses)"
        );
        metrics::describe_gauge!(
            "prime_chain_fba_clearing_price",
            "Most recent clearing price per market (labeled by market_id)"
        );
        metrics::describe_gauge!(
            "prime_chain_fba_matched_size",
            "Matched size at last FBA tick (labeled by market_id)"
        );
        metrics::describe_counter!(
            "prime_chain_liquidation_claims_received_total",
            "Liquidation claims submitted (chain-wide aggregate)"
        );
        metrics::describe_counter!(
            "prime_chain_liquidation_auctions_settled_total",
            "Liquidation auctions settled (chain-wide aggregate)"
        );
        metrics::describe_gauge!(
            "prime_chain_liquidator_count",
            "Number of bonded liquidators (no per-bond labels)"
        );
        metrics::describe_counter!(
            "prime_chain_dkg_ceremonies_started_total",
            "DKG ceremonies started (labeled by epoch)"
        );
        metrics::describe_counter!(
            "prime_chain_dkg_ceremonies_completed_total",
            "DKG ceremonies completed (labeled by epoch)"
        );
        metrics::describe_counter!(
            "prime_chain_dkg_complaints_total",
            "DKG slashing complaints raised (labeled by epoch)"
        );
        metrics::describe_counter!(
            "prime_chain_state_proofs_attached_total",
            "State-transition proofs attached to blocks"
        );
        metrics::describe_histogram!(
            "prime_chain_state_proof_generation_seconds",
            "Wall-clock time to generate a state-transition proof"
        );
        metrics::describe_gauge!(
            "prime_chain_privacy_activation_height",
            "Configured activation height for privacy mode (0 if unset)"
        );
    }
}
