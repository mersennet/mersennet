use prime_chain::crypto::{
    SignedTransaction, generate_keypair, recover_signer, sign_transaction, tx_signing_hash,
};
use prime_chain::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};

fn sample_tx(from: Address, chain_id: u64) -> Transaction {
    Transaction {
        from,
        to: Some(Address::from_slice(&[0xBB; 20])),
        value: U256::from(1_000u64),
        data: Bytes::new(),
        gas_limit: 21_000,
        gas_price: U256::from(1u64),
        nonce: 0,
        chain_id: Some(chain_id),
        signature: None,
    }
}

#[test]
fn sign_and_recover_roundtrip() {
    let (key, addr) = generate_keypair();
    let tx = sample_tx(addr, 7919);
    let signed = sign_transaction(&tx, &key);
    let recovered = recover_signer(&signed).expect("recovery should succeed");
    assert_eq!(recovered, addr);
}

#[test]
fn different_keys_produce_different_signatures() {
    let (key_a, addr_a) = generate_keypair();
    let (key_b, addr_b) = generate_keypair();

    let tx_a = sample_tx(addr_a, 7919);
    let tx_b = sample_tx(addr_b, 7919);

    let signed_a = sign_transaction(&tx_a, &key_a);
    let signed_b = sign_transaction(&tx_b, &key_b);

    assert_ne!(signed_a.r, signed_b.r);
}

#[test]
fn tampered_data_breaks_recovery() {
    let (key, addr) = generate_keypair();
    let tx = sample_tx(addr, 7919);
    let signed = sign_transaction(&tx, &key);

    let mut tampered_tx = signed.tx.clone();
    tampered_tx.value = U256::from(9_999u64);

    let tampered_signed = SignedTransaction {
        tx: tampered_tx,
        v: signed.v,
        r: signed.r,
        s: signed.s,
    };

    let recovered =
        recover_signer(&tampered_signed).expect("recovery succeeds but address differs");
    assert_ne!(
        recovered, addr,
        "tampered tx must not recover to original signer"
    );
}

#[test]
fn chain_id_protection() {
    let (key, addr) = generate_keypair();
    let tx_chain_a = sample_tx(addr, 1);
    let tx_chain_b = sample_tx(addr, 2);

    let hash_a = tx_signing_hash(&tx_chain_a);
    let hash_b = tx_signing_hash(&tx_chain_b);
    assert_ne!(
        hash_a, hash_b,
        "different chain_id must produce different hashes"
    );

    let signed_a = sign_transaction(&tx_chain_a, &key);
    let recovered_a = recover_signer(&signed_a).expect("recovery on chain A");
    assert_eq!(recovered_a, addr);

    let cross_chain = SignedTransaction {
        tx: tx_chain_b.clone(),
        v: signed_a.v,
        r: signed_a.r,
        s: signed_a.s,
    };
    let recovered_cross = recover_signer(&cross_chain);
    match recovered_cross {
        Ok(address) => assert_ne!(
            address, addr,
            "cross-chain replay must not recover original signer"
        ),
        Err(_) => {} // also acceptable: recovery fails entirely
    }
}
