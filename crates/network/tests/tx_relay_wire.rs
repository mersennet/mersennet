//! Regression tests for the transaction-relay wire format.
//!
//! A relayed transaction must survive the `Transaction -> WireTx -> JSON ->
//! WireTx -> Transaction` round-trip with its signature intact. Otherwise a
//! receiving validator sees an unsigned tx, rejects it, and externally
//! submitted transactions (faucet claims, trades) never get mined — the bug
//! that left faucet claims "successful" but balances empty on the live fleet.

use mersennet::crypto::{SignedTransaction, generate_keypair, recover_signer, sign_transaction};
use mersennet::engine::Transaction;
use mersennet_network::p2p::{WireTx, tx_to_wire, wire_to_tx};
use revm::primitives::{Bytes, U256};

fn sample_signed_tx() -> Transaction {
    let (key, from) = generate_keypair();
    let (_to_key, to) = generate_keypair();
    let tx = Transaction {
        from,
        to: Some(to),
        value: U256::from(1_000u64) * U256::from(10u64).pow(U256::from(18)),
        data: Bytes::new(),
        gas_limit: 21_000,
        gas_price: U256::from(1u64),
        nonce: 0,
        chain_id: Some(131_071),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
    };
    // `sign_transaction` returns a SignedTransaction whose inner `tx`
    // carries the populated `signature` field.
    sign_transaction(&tx, &key).tx
}

#[test]
fn wire_roundtrip_preserves_signature() {
    let tx = sample_signed_tx();
    assert!(tx.signature.is_some(), "fixture must be signed");

    let wire = tx_to_wire(&tx);
    let bytes = serde_json::to_vec(&wire).expect("serialize WireTx");
    let decoded: WireTx = serde_json::from_slice(&bytes).expect("deserialize WireTx");
    let relayed = wire_to_tx(&decoded).expect("reconstruct tx");

    assert_eq!(
        relayed.signature, tx.signature,
        "signature must survive the gossip round-trip"
    );
    assert_eq!(relayed.from, tx.from);
    assert_eq!(relayed.nonce, tx.nonce);
    assert_eq!(relayed.value, tx.value);
    assert_eq!(relayed.chain_id, tx.chain_id);
}

#[test]
fn relayed_tx_recovers_original_signer() {
    let tx = sample_signed_tx();
    let expected = tx.from;

    let wire = tx_to_wire(&tx);
    let bytes = serde_json::to_vec(&wire).unwrap();
    let decoded: WireTx = serde_json::from_slice(&bytes).unwrap();
    let relayed = wire_to_tx(&decoded).unwrap();

    let (r, s, v) = relayed.signature.expect("relayed tx must be signed");
    let signed = SignedTransaction {
        tx: relayed.clone(),
        v: U256::from(v),
        r,
        s,
        tx_type: 0,
    };
    let recovered = recover_signer(&signed).expect("signature must verify");
    assert_eq!(
        recovered, expected,
        "validator must recover the original signer from a relayed tx"
    );
}

#[test]
fn block_wire_roundtrip_preserves_rewards() {
    use mersennet::consensus::Reward;
    use mersennet::engine::Engine;
    use mersennet_network::p2p::{WireBlock, block_to_wire, wire_to_block};
    use revm::primitives::Address;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let mut engine = Engine::new_with_state(131_071, dir.path());
    let (_k, alice) = generate_keypair();
    engine.fund_account(alice, U256::from(5_000_000u64), 0);
    let (_k2, bob) = generate_keypair();
    engine
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .unwrap();
    let mut block = engine.execute_block().unwrap();
    // Inject a reward so we can assert it survives the round-trip even
    // if the test engine produced none on its own.
    block.rewards.push(Reward {
        address: Address::from_slice(&[0x99; 20]),
        amount: U256::from(7u64),
    });

    let wire = block_to_wire(&block);
    let bytes = serde_json::to_vec(&wire).unwrap();
    let decoded: WireBlock = serde_json::from_slice(&bytes).unwrap();
    let relayed = wire_to_block(&decoded).expect("reconstruct block");

    assert_eq!(relayed.rewards.len(), block.rewards.len());
    let injected = relayed
        .rewards
        .iter()
        .find(|r| r.address == Address::from_slice(&[0x99; 20]))
        .expect("injected reward must survive round-trip");
    assert_eq!(injected.amount, U256::from(7u64));
    assert_eq!(relayed.transactions.len(), block.transactions.len());
    assert_eq!(relayed.state_root, block.state_root);
}

#[test]
fn legacy_wire_without_signature_is_unsigned() {
    // A WireTx serialized by an older node carries no signature fields.
    // It must still deserialize (serde defaults) and yield an unsigned tx
    // rather than failing to parse.
    let legacy = r#"{
        "from":"0000000000000000000000000000000000000001",
        "to":null,
        "value":"0",
        "data":"",
        "gas_limit":21000,
        "gas_price":"1",
        "nonce":0,
        "chain_id":131071
    }"#;
    let wire: WireTx = serde_json::from_str(legacy).expect("legacy WireTx must deserialize");
    let tx = wire_to_tx(&wire).expect("reconstruct legacy tx");
    assert!(tx.signature.is_none());
    assert_eq!(tx.tx_type, 0);
}
