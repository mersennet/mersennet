//! The protocol call lists every consensus switch a client schedules from:
//! the mark switch is null until a release sets its height.

use mersennet::engine::Engine;
use mersennet_rpc::rpc_router::route;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn the_protocol_call_reports_the_mark_switch_height_once_set() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());
    let protocol = route("mersennet_orders_getProtocol", json!([]), &mut engine).unwrap();
    assert!(protocol["switches"]["markPriceHeight"].is_null());

    engine.set_mark_price_height(Some(2_900_000));
    let protocol = route("mersennet_orders_getProtocol", json!([]), &mut engine).unwrap();
    assert_eq!(protocol["switches"]["markPriceHeight"], json!(2_900_000));
}
