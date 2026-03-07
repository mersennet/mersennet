use prime_chain::engine::Engine;
use revm::primitives::{Address, Bytes, U256};
use tempfile::tempdir;

#[test]
fn block_execution_produces_receipts() {
    let dir = tempdir().expect("temp dir");
    let mut engine = Engine::new_with_state(7919, dir.path());

    let alice = Address::from_slice(&[0x11; 20]);
    let bob = Address::from_slice(&[0x22; 20]);
    engine.fund_account(alice, U256::from(2_000_000u64), 0);

    engine
        .transfer(alice, bob, U256::from(1_000u64), 21_000, U256::from(1u64), 0)
        .expect("transfer");

    let contract_creation = Bytes::from_static(&[
        0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a,
        0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ]);

    engine
        .deploy_contract(alice, contract_creation, 1_000_000, U256::from(1u64), 1, U256::ZERO)
        .expect("deploy");

    let block = engine.execute_block().expect("execute block");
    assert_eq!(block.transactions.len(), 2);
    assert_eq!(block.receipts.len(), 2);
    assert!(block.receipts[0].success);
    assert_eq!(block.receipts[0].gas_used, 21_000);
    assert!(!block.state_root.is_zero());
}
