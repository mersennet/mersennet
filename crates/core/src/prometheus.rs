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
        metrics::describe_gauge!("prime_chain_height", "Latest committed block height");

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
        metrics::describe_counter!("prime_chain_orders_filled", "Total orders fully filled");
        metrics::describe_counter!("prime_chain_orders_cancelled", "Total orders cancelled");
        metrics::describe_counter!("prime_chain_trades_executed", "Total trade fills executed");
        metrics::describe_gauge!(
            "prime_chain_insurance_fund_balance",
            "Insurance fund balance"
        );
        metrics::describe_gauge!("prime_chain_markets_active", "Number of active markets");

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
        metrics::describe_counter!("prime_chain_rpc_requests", "Total RPC requests by method");
        metrics::describe_counter!(
            "prime_chain_rpc_errors",
            "Total RPC errors by method and code"
        );
        metrics::describe_histogram!(
            "prime_chain_rpc_duration_seconds",
            "RPC request duration in seconds by method"
        );
    }
}
