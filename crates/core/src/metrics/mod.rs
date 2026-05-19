// Re-export from the canonical prometheus module.
// This file exists for backward compatibility; the primary implementation
// lives in src/prometheus/prometheus.rs.
pub use crate::prometheus::{MetricsRegistry, handle, init};
