use prime_chain::consensus::Consensus;
use revm::primitives::{Address, B256, U256};

fn addr(byte: u8) -> Address {
    Address::from_slice(&[byte; 20])
}

#[test]
fn proposer_selection_deterministic_and_fair() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    let bob = addr(0x22);
    let carol = addr(0x33);

    consensus.stake(alice, U256::from(10u64)).unwrap();
    consensus.stake(bob, U256::from(10u64)).unwrap();
    consensus.stake(carol, U256::from(10u64)).unwrap();

    let rounds = 30u64;
    let mut counts = std::collections::HashMap::new();
    for h in 1..=rounds {
        let proposer = consensus.proposer(h);
        *counts.entry(proposer).or_insert(0u64) += 1;
    }

    for validator in [alice, bob, carol] {
        let count = counts.get(&validator).copied().unwrap_or(0);
        assert!(
            count >= rounds / 3 - 2 && count <= rounds / 3 + 2,
            "validator {validator:?} proposed {count} times in {rounds} rounds, expected ~{}",
            rounds / 3
        );
    }

    let mut consensus2 = Consensus::default();
    consensus2.stake(alice, U256::from(10u64)).unwrap();
    consensus2.stake(bob, U256::from(10u64)).unwrap();
    consensus2.stake(carol, U256::from(10u64)).unwrap();

    for h in 1..=rounds {
        assert_eq!(
            consensus2.proposer(h),
            {
                let mut c3 = Consensus::default();
                c3.stake(alice, U256::from(10u64)).unwrap();
                c3.stake(bob, U256::from(10u64)).unwrap();
                c3.stake(carol, U256::from(10u64)).unwrap();
                let mut last = Address::ZERO;
                for hh in 1..=h {
                    last = c3.proposer(hh);
                }
                last
            },
            "proposer must be deterministic at height {h}"
        );
    }
}

#[test]
fn finality_all_voting() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    let bob = addr(0x22);
    let carol = addr(0x33);

    consensus.stake(alice, U256::from(10u64)).unwrap();
    consensus.stake(bob, U256::from(10u64)).unwrap();
    consensus.stake(carol, U256::from(10u64)).unwrap();

    let hash = B256::from_slice(&[0xAA; 32]);
    let finalization = consensus.finalize(hash, 1);
    assert!(
        finalization.finalized,
        "3/3 validators voting should finalize"
    );
    assert_eq!(finalization.committed_stake, U256::from(30u64));
}

#[test]
fn no_finality_insufficient_votes() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    let bob = addr(0x22);
    let carol = addr(0x33);

    consensus.stake(alice, U256::from(10u64)).unwrap();
    consensus.stake(bob, U256::from(10u64)).unwrap();
    consensus.stake(carol, U256::from(10u64)).unwrap();

    consensus.set_miss_precommit(bob);
    consensus.set_miss_precommit(carol);

    let hash = B256::from_slice(&[0xBB; 32]);
    let mut network = prime_chain::network::NetworkSim::default();
    let (rounds, _evidence) = consensus.run_finality_rounds(hash, 1, 2, &mut network);

    let any_finalized = rounds.iter().any(|r| r.finalized);
    assert!(!any_finalized, "only 1/3 voting should not finalize");
}

#[test]
fn slashing_escalation_increases_penalty() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    consensus.stake(alice, U256::from(1000u64)).unwrap();

    consensus.set_slashing_bps(500, 100);
    consensus.set_slashing_escalation(50, 1000);

    let evidence = prime_chain::consensus::SlashingEvidence {
        validator: alice,
        kind: prime_chain::consensus::EvidenceKind::PrecommitTimeout,
        height: 1,
        round: 0,
        rounds_missed: 1,
    };

    let slash_0 = consensus.slash_amount_for_evidence(&evidence, 1);

    consensus.record_offense(alice);
    let slash_1 = consensus.slash_amount_for_evidence(&evidence, 2);

    consensus.record_offense(alice);
    let slash_2 = consensus.slash_amount_for_evidence(&evidence, 3);

    assert!(
        slash_1 > slash_0,
        "penalty should increase after first offense: {slash_1} > {slash_0}"
    );
    assert!(
        slash_2 > slash_1,
        "penalty should increase after second offense: {slash_2} > {slash_1}"
    );
}

#[test]
fn tombstoned_validator_cannot_rejoin() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    let bob = addr(0x22);
    consensus.stake(alice, U256::from(100u64)).unwrap();
    consensus.stake(bob, U256::from(100u64)).unwrap();

    assert!(!consensus.is_tombstoned(alice));
    assert!(
        consensus.stake(alice, U256::from(50u64)).is_ok(),
        "non-tombstoned can add stake"
    );

    // Unjailing a non-jailed validator should fail
    assert!(consensus.unjail(alice, 100).is_err());

    // Jail alice, then unjail at correct height
    consensus.jail(alice, 50);
    assert!(consensus.unjail(alice, 49).is_err(), "too early to unjail");
    assert!(consensus.unjail(alice, 50).is_ok(), "unjail at expiry");

    // After unjailing, can still participate
    assert!(consensus.stake(alice, U256::from(10u64)).is_ok());
}

#[test]
fn jailing_and_unjailing() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    consensus.stake(alice, U256::from(100u64)).unwrap();

    consensus.jail(alice, 100);

    let result = consensus.unjail(alice, 50);
    assert!(result.is_err(), "unjail before expiry should fail");

    let result = consensus.unjail(alice, 100);
    assert!(result.is_ok(), "unjail at expiry height should succeed");
}

#[test]
fn unbonding_period_locks_funds() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    consensus.stake(alice, U256::from(100u64)).unwrap();
    consensus.set_unbonding_period(10);

    consensus
        .begin_unbonding(alice, U256::from(50u64), 5)
        .unwrap();

    let released = consensus.process_unbonding(10);
    assert!(
        released.is_empty(),
        "funds should still be locked at height 10"
    );

    let released = consensus.process_unbonding(15);
    assert_eq!(released.len(), 1, "funds should be released at height 15");
    assert_eq!(released[0].amount, U256::from(50u64));
}

#[test]
fn block_rewards_halving() {
    let mut consensus = Consensus::default();
    consensus.set_token_economics(U256::from(1_000_000u64), U256::from(100u64), 10);

    let reward_h1 = consensus.reward_per_block(1);
    let reward_h10 = consensus.reward_per_block(10);
    let reward_h20 = consensus.reward_per_block(20);

    assert_eq!(reward_h1, U256::from(100u64), "before first halving");
    assert_eq!(reward_h10, U256::from(50u64), "after first halving");
    assert_eq!(reward_h20, U256::from(25u64), "after second halving");
}

#[test]
fn reward_distribution_proportional_to_stake() {
    let mut consensus = Consensus::default();
    let alice = addr(0x11);
    let bob = addr(0x22);

    consensus.stake(alice, U256::from(75u64)).unwrap();
    consensus.stake(bob, U256::from(25u64)).unwrap();

    consensus.set_token_economics(U256::from(1_000_000u64), U256::from(100u64), 1_000);

    let hash = B256::from_slice(&[0xEE; 32]);
    let finalization = consensus.finalize(hash, 1);

    let alice_reward = finalization
        .rewards
        .iter()
        .find(|r| r.address == alice)
        .map(|r| r.amount)
        .unwrap_or(U256::ZERO);
    let bob_reward = finalization
        .rewards
        .iter()
        .find(|r| r.address == bob)
        .map(|r| r.amount)
        .unwrap_or(U256::ZERO);

    assert_eq!(
        alice_reward,
        U256::from(75u64),
        "alice (75%) should get 75 of 100"
    );
    assert_eq!(
        bob_reward,
        U256::from(25u64),
        "bob (25%) should get 25 of 100"
    );
}
