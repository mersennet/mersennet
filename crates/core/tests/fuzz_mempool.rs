use mersennet::engine::Transaction;
use mersennet::mempool::Mempool;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use revm::primitives::{Address, Bytes, U256};

#[test]
fn mempool_randomized_inserts_do_not_exceed_limits() {
    let mut rng = StdRng::seed_from_u64(42);
    let max_total = 50;
    let max_per_sender = 10;
    let mut mempool = Mempool::with_limits(max_total, max_per_sender, 500);
    let base_fee = U256::from(1);

    for _ in 0..500 {
        let sender = Address::from_slice(&rng.r#gen::<[u8; 20]>());
        let tx = Transaction {
            from: sender,
            to: Some(Address::from_slice(&rng.r#gen::<[u8; 20]>())),
            value: U256::from(rng.gen_range(0u64..1000u64)),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(rng.gen_range(1u64..1000u64)),
            nonce: rng.gen_range(0u64..100u64),
            chain_id: Some(131071),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
        };
        let _ = mempool.insert(tx, base_fee, 0);
        assert!(mempool.len() <= max_total);
    }
}
