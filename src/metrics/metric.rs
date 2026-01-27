use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;

static PROM_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

/// Initialize the Prometheus exporter and return a static handle.
/// Safe to call multiple times; first call installs the exporter.
pub fn init() -> &'static PrometheusHandle {
    PROM_HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("failed to install Prometheus metrics exporter")
    })
}

/// Get the handle if already initialized.
pub fn handle() -> Option<&'static PrometheusHandle> {
    PROM_HANDLE.get()
}
