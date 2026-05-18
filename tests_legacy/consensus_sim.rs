use prime_chain::consensus::{Consensus, EvidenceKind};
use prime_chain::network::NetworkSim;
use revm::primitives::{Address, B256, U256};

#[test]
fn consensus_timeout_evidence_produced() {
    let mut consensus = Consensus::default();
    let alice = Address::from_slice(&[0x11; 20]);
    let bob = Address::from_slice(&[0x22; 20]);
    let carol = Address::from_slice(&[0x33; 20]);

    consensus.stake(alice, U256::from(10u64)).expect("stake");
    consensus.stake(bob, U256::from(10u64)).expect("stake");
    consensus.stake(carol, U256::from(10u64)).expect("stake");

    consensus.set_miss_precommit(carol);
    let mut network = NetworkSim::default();
    let block_hash = B256::from_slice(&[0x44; 32]);
    let (_rounds, evidence) = consensus.run_finality_rounds(block_hash, 1, 3, &mut network);

    assert!(
        evidence
            .iter()
            .any(|e| matches!(e.kind, EvidenceKind::PrecommitTimeout)),
        "expected timeout evidence"
    );
    let max_streak = evidence
        .iter()
        .filter(|e| matches!(e.kind, EvidenceKind::PrecommitTimeout))
        .map(|e| e.rounds_missed)
        .max()
        .unwrap_or(0);
    assert!(max_streak >= 1);
}
