//! End-to-end circuit integration tests.
//!
//! These exercise the same public-input layouts that the production
//! Noir circuits expose. The mock path stays as the deterministic
//! baseline, and the `prover`-feature round-trip test checks the real
//! Barretenberg-backed wiring when the external toolchain is
//! configured. Every circuit's public-input shape must remain stable
//! byte-for-byte.

#[cfg(feature = "prover")]
use mersennet_zkp::noir::{WitnessInputs, WitnessValue, default_prover, default_verifier};
use mersennet_zkp::{
    Fr,
    merkle::MerkleTree,
    noir::{Circuit, MockVerifier, Verifier},
    note::Note,
    nullifier::Nullifier,
    poseidon::Poseidon,
};
#[cfg(feature = "prover")]
use std::env;

fn poseidon() -> Poseidon {
    Poseidon::default()
}

fn make_note(value: u128, owner: u64, rho: u64, psi: u64) -> Note {
    Note {
        value,
        asset_id: 0,
        owner_pk: Fr::from_u64(owner),
        rho: Fr::from_u64(rho),
        psi: Fr::from_u64(psi),
    }
}

/// Spend circuit end-to-end:
/// (anchor, nullifier, new_commitment, public_amount) → mock proof → verify.
#[test]
fn spend_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();

    let mut tree = MerkleTree::new();
    let spent = make_note(1000, 42, 11, 22);
    let _idx = tree.insert(spent.commit(&p).0);
    let anchor = tree.root();
    let spend_sk = Fr::from_u64(0xdeadbeef);
    let nullifier = spent.nullifier(&p, &spend_sk);

    let change = make_note(700, 42, 13, 24);
    let new_commitment = change.commit(&p).0;
    let public_amount = Fr::from_u64(300);

    let public_inputs = vec![anchor, nullifier.0, new_commitment, public_amount];
    let proof = v.prove(Circuit::Spend, public_inputs.clone());
    v.verify(&proof, Circuit::Spend, &public_inputs)
        .expect("spend proof must verify");
}

/// Output (shield-in) circuit:
/// (commitment, asset_id, public_amount).
#[test]
fn output_shield_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();
    let note = make_note(2_000_000, 7, 1, 2);
    let public_inputs = vec![
        note.commit(&p).0,
        Fr::from_u64(note.asset_id as u64),
        Fr::from_u64(note.value as u64),
    ];
    let proof = v.prove(Circuit::Output, public_inputs.clone());
    v.verify(&proof, Circuit::Output, &public_inputs)
        .expect("output proof must verify");
}

/// Join-split (2-in, 2-out + public_amount) end-to-end.
#[test]
fn join_split_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();
    let mut tree = MerkleTree::new();

    let in1 = make_note(500, 1, 10, 20);
    let in2 = make_note(800, 1, 11, 21);
    let _ = tree.insert(in1.commit(&p).0);
    let _ = tree.insert(in2.commit(&p).0);
    let root = tree.root();

    let sk = Fr::from_u64(0x1234);
    let n1 = in1.nullifier(&p, &sk).0;
    let n2 = in2.nullifier(&p, &sk).0;

    let out1 = make_note(900, 2, 30, 40);
    let out2 = make_note(400, 1, 31, 41);
    let cm1 = out1.commit(&p).0;
    let cm2 = out2.commit(&p).0;
    let public_amount = Fr::ZERO;

    let public_inputs = vec![root, n1, n2, cm1, cm2, public_amount];
    let proof = v.prove(Circuit::JoinSplit, public_inputs.clone());
    v.verify(&proof, Circuit::JoinSplit, &public_inputs)
        .expect("join-split proof must verify");
}

/// Order-place end-to-end. Public inputs:
/// (root, nullifier, new_commitment, market_id, side_hash,
///  price_band, size_band, oracle_price, imm_required).
#[test]
fn order_place_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();
    let mut tree = MerkleTree::new();

    let collateral = make_note(10_000, 9, 99, 100);
    let _ = tree.insert(collateral.commit(&p).0);
    let root = tree.root();

    let sk = Fr::from_u64(0xaaaaaaaa);
    let nullifier = collateral.nullifier(&p, &sk).0;

    let remaining = make_note(9_500, 9, 199, 200);
    let new_commitment = remaining.commit(&p).0;

    let market_id = Fr::from_u64(1); // PRIM-PERP
    let side = Fr::from_u64(1); // ask
    let side_salt = Fr::from_u64(0xdead);
    let side_hash = p.hash_two(&side, &side_salt);

    let price_band = Fr::from_u64(450); // $4.50 tier
    let size_band = Fr::from_u64(100); // 100 PRIM tier
    let oracle_price = Fr::from_u64(449);
    let imm_required = Fr::from_u64(500); // 5% margin on a 100 PRIM order

    let public_inputs = vec![
        root,
        nullifier,
        new_commitment,
        market_id,
        side_hash,
        price_band,
        size_band,
        oracle_price,
        imm_required,
    ];
    let proof = v.prove(Circuit::OrderPlace, public_inputs.clone());
    v.verify(&proof, Circuit::OrderPlace, &public_inputs)
        .expect("order-place proof must verify");
}

/// Liquidate-claim end-to-end. Public inputs:
/// (root, market_id, oracle_price, liquidator_id, claim_tag).
#[test]
fn liquidate_claim_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();
    let mut tree = MerkleTree::new();
    let victim_position = make_note(1, 13, 7, 8);
    let _ = tree.insert(victim_position.commit(&p).0);
    let root = tree.root();
    let market_id = Fr::from_u64(2); // ETH-PERP
    let oracle_price = Fr::from_u64(3200);
    let liquidator_id = Fr::from_u64(0xbeef);
    let claim_tag = p.hash_two(&victim_position.commit(&p).0, &oracle_price);
    let public_inputs = vec![root, market_id, oracle_price, liquidator_id, claim_tag];
    let proof = v.prove(Circuit::LiquidateClaim, public_inputs.clone());
    v.verify(&proof, Circuit::LiquidateClaim, &public_inputs)
        .expect("liquidate-claim proof must verify");
}

/// Liquidate-execute end-to-end. Public inputs:
/// (root, victim_nullifier, bounty_commitment, insurance_commitment,
///  winning_bid, market_id, oracle_price).
#[test]
fn liquidate_execute_end_to_end() {
    let p = poseidon();
    let v = MockVerifier::new();
    let mut tree = MerkleTree::new();

    let victim_pos = make_note(1_000_000, 13, 7, 8);
    let _ = tree.insert(victim_pos.commit(&p).0);
    let root = tree.root();

    let victim_sk = Fr::from_u64(0xc0ffee);
    let victim_nullifier = victim_pos.nullifier(&p, &victim_sk).0;

    let bounty = make_note(50_000, 0xbeef, 1, 2);
    let insurance = make_note(950_000, 0, 3, 4);
    let bounty_commitment = bounty.commit(&p).0;
    let insurance_commitment = insurance.commit(&p).0;

    let winning_bid = Fr::from_u64(95);
    let market_id = Fr::from_u64(2);
    let oracle_price = Fr::from_u64(3200);

    let public_inputs = vec![
        root,
        victim_nullifier,
        bounty_commitment,
        insurance_commitment,
        winning_bid,
        market_id,
        oracle_price,
    ];
    let proof = v.prove(Circuit::LiquidateExecute, public_inputs.clone());
    v.verify(&proof, Circuit::LiquidateExecute, &public_inputs)
        .expect("liquidate-execute proof must verify");
}

/// Defense in depth: verify that all six circuit-id variants reject
/// proofs whose vk_hash is wrong.
#[test]
fn circuits_reject_cross_circuit_vk_substitution() {
    let v = MockVerifier::new();
    let circuits = [
        Circuit::Spend,
        Circuit::Output,
        Circuit::JoinSplit,
        Circuit::OrderPlace,
        Circuit::LiquidateClaim,
        Circuit::LiquidateExecute,
    ];
    let inputs = vec![Fr::from_u64(1)];
    for &c in &circuits {
        let mut proof = v.prove(c, inputs.clone());
        // Substitute the vk_hash with the wrong circuit's. Pick one
        // that isn't c.
        let wrong = circuits.iter().copied().find(|x| *x != c).unwrap();
        proof.vk_hash = MockVerifier::vk_hash_for(wrong);
        let err = v
            .verify(&proof, c, &inputs)
            .expect_err("substituted vk_hash must be rejected");
        assert!(matches!(
            err,
            mersennet_zkp::noir::VerifyError::VerifyingKeyMismatch
        ));
    }
}

/// Defense in depth: confirm a nullifier from one keypair cannot be
/// computed by a different keypair for the same note.
#[test]
fn nullifier_is_keyed_to_owner() {
    let p = poseidon();
    let note = make_note(1000, 42, 11, 22);
    let n_alice = note.nullifier(&p, &Fr::from_u64(1));
    let n_bob = note.nullifier(&p, &Fr::from_u64(2));
    assert_ne!(n_alice, n_bob);
}

/// Defense in depth: the same nullifier inserted twice into the set
/// fails the second time. This is the double-spend protection.
#[test]
fn nullifier_set_double_spend() {
    let mut s = mersennet_zkp::NullifierSet::new();
    let n = Nullifier(Fr::from_u64(0xfeedface));
    assert!(s.insert(n));
    assert!(!s.insert(n));
}

#[cfg(feature = "prover")]
#[test]
fn real_toolchain_spend_and_order_place_round_trip_when_configured() {
    if !real_noir_toolchain_configured() {
        return;
    }

    let prover = default_prover();
    let verifier = default_verifier();
    let p = poseidon();

    let mut spend_tree = MerkleTree::new();
    let spent = make_note(1000, 42, 11, 22);
    let spend_index = spend_tree.insert(spent.commit(&p).0);
    let spend_membership = spend_tree.prove(spend_index);
    let spend_sk = Fr::from_u64(0xdead_beef);
    let nullifier = spent.nullifier(&p, &spend_sk);
    let change = make_note(700, 42, 13, 24);
    let spend_public_inputs = vec![
        spend_tree.root(),
        nullifier.0,
        change.commit(&p).0,
        Fr::from_u64(300),
    ];
    let spend_witness = spend_witness(&spent, &change, &spend_membership, spend_sk);
    let spend_proof = prover
        .prove(Circuit::Spend, spend_public_inputs.clone(), &spend_witness)
        .expect("configured Noir toolchain should produce a spend proof");
    assert_ne!(
        spend_proof.vk_hash,
        MockVerifier::vk_hash_for(Circuit::Spend)
    );
    verifier
        .verify(&spend_proof, Circuit::Spend, &spend_public_inputs)
        .expect("configured verifier should accept the spend proof");

    let mut order_tree = MerkleTree::new();
    let collateral = make_note(10_000, 9, 99, 100);
    let order_index = order_tree.insert(collateral.commit(&p).0);
    let order_membership = order_tree.prove(order_index);
    let order_sk = Fr::from_u64(0xaaaaaaaa);
    let remaining = make_note(9_500, 9, 199, 200);
    let side = Fr::from_u64(1);
    let side_salt = Fr::from_u64(0xdead);
    let order_public_inputs = vec![
        order_tree.root(),
        collateral.nullifier(&p, &order_sk).0,
        remaining.commit(&p).0,
        Fr::from_u64(1),
        p.hash_two(&side, &side_salt),
        Fr::from_u64(450),
        Fr::from_u64(100),
        Fr::from_u64(449),
        Fr::from_u64(500),
    ];
    let order_witness = order_place_witness(
        &collateral,
        &remaining,
        &order_membership,
        order_sk,
        side_salt,
        side,
    );
    let order_proof = prover
        .prove(
            Circuit::OrderPlace,
            order_public_inputs.clone(),
            &order_witness,
        )
        .expect("configured Noir toolchain should produce an order proof");
    assert_ne!(
        order_proof.vk_hash,
        MockVerifier::vk_hash_for(Circuit::OrderPlace)
    );
    verifier
        .verify(&order_proof, Circuit::OrderPlace, &order_public_inputs)
        .expect("configured verifier should accept the order proof");
}

#[cfg(feature = "prover")]
fn real_noir_toolchain_configured() -> bool {
    env::var("PRIME_BB_PROVE_ADAPTER").is_ok() && env::var("PRIME_BB_VERIFY_ADAPTER").is_ok()
}

#[cfg(feature = "prover")]
fn spend_witness(
    spent: &Note,
    output: &Note,
    membership: &mersennet_zkp::MerkleProof,
    spend_sk: Fr,
) -> WitnessInputs {
    let mut witness = WitnessInputs::new();
    witness.insert("root", membership.root(&poseidon()));
    witness.insert("nullifier", spent.nullifier(&poseidon(), &spend_sk).0);
    witness.insert("new_commitment", output.commit(&poseidon()).0);
    witness.insert(
        "public_amount",
        Fr::from_u64((spent.value - output.value) as u64),
    );
    witness.insert(
        "spent_note",
        WitnessValue::structure([("note", note_witness(spent))]),
    );
    witness.insert(
        "output_note",
        WitnessValue::structure([("note", note_witness(output))]),
    );
    witness.insert(
        "merkle_path",
        WitnessValue::array(
            membership
                .siblings
                .iter()
                .copied()
                .map(WitnessValue::from)
                .collect(),
        ),
    );
    witness.insert(
        "merkle_index_bits",
        WitnessValue::from(index_bits(membership.index)),
    );
    witness.insert("spend_sk", spend_sk);
    witness
}

#[cfg(feature = "prover")]
fn order_place_witness(
    spent: &Note,
    output: &Note,
    membership: &mersennet_zkp::MerkleProof,
    spend_sk: Fr,
    side_salt: Fr,
    side: Fr,
) -> WitnessInputs {
    let p = poseidon();
    let mut witness = WitnessInputs::new();
    witness.insert("root", membership.root(&p));
    witness.insert("nullifier", spent.nullifier(&p, &spend_sk).0);
    witness.insert("new_commitment", output.commit(&p).0);
    witness.insert("market_id", Fr::from_u64(1));
    witness.insert("side_hash", p.hash_two(&side, &side_salt));
    witness.insert("price_band", Fr::from_u64(450));
    witness.insert("size_band", Fr::from_u64(100));
    witness.insert("oracle_price", Fr::from_u64(449));
    witness.insert("imm_required", Fr::from_u64(500));
    witness.insert("spent", note_witness(spent));
    witness.insert("output", note_witness(output));
    witness.insert(
        "spent_path",
        WitnessValue::array(
            membership
                .siblings
                .iter()
                .copied()
                .map(WitnessValue::from)
                .collect(),
        ),
    );
    witness.insert(
        "spent_index_bits",
        WitnessValue::from(index_bits(membership.index)),
    );
    witness.insert("spend_sk", spend_sk);
    witness.insert("side_salt", side_salt);
    witness.insert("side", side);
    witness
}

#[cfg(feature = "prover")]
fn note_witness(note: &Note) -> WitnessValue {
    WitnessValue::structure([
        ("value_lo", Fr::from_u64(note.value as u64)),
        ("value_hi", Fr::from_u64((note.value >> 64) as u64)),
        ("asset_id", Fr::from_u64(note.asset_id as u64)),
        ("owner_pk", note.owner_pk),
        ("rho", note.rho),
        ("psi", note.psi),
    ])
}

#[cfg(feature = "prover")]
fn index_bits(index: u64) -> [u64; 32] {
    let mut bits = [0u64; 32];
    for (bit, slot) in bits.iter_mut().enumerate() {
        *slot = (index >> bit) & 1;
    }
    bits
}
