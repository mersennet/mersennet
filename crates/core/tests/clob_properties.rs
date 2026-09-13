//! Property tests for the on-chain CLOB (`MersennetOrdersState`).
//!
//! Random sequences of limit orders (GTC/IOC, both sides, prices around a
//! mid) and cancels go through `submit_order` (matching, TIF and margin —
//! `place_order` is the raw book insert) on a fresh market; after every step the book
//! must satisfy the invariants a matching engine cannot violate without a
//! bug: no crossed book, every resting id is a live order with a non-zero
//! size at the level it sits on, per-account open-order lists agree with the
//! book, and positions net to zero across all accounts (every fill has one
//! buyer and one seller).

use mersennet::mersennet_orders::{MersennetOrdersState, OrderId, Side, TimeInForce};
use proptest::prelude::*;
use revm::primitives::{Address, U256};
use std::collections::HashSet;

#[derive(Debug, Clone)]
enum Action {
    Place {
        owner: u8,
        buy: bool,
        price: u64,
        size: u64,
        ioc: bool,
    },
    Cancel {
        nth: usize,
    },
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        4 => (0u8..4, any::<bool>(), 90u64..=110, 1u64..=20, any::<bool>())
            .prop_map(|(owner, buy, price, size, ioc)| Action::Place { owner, buy, price, size, ioc }),
        1 => (0usize..64).prop_map(|nth| Action::Cancel { nth }),
    ]
}

fn addr(i: u8) -> Address {
    Address::from_slice(&[i + 1; 20])
}

fn check_invariants(state: &MersennetOrdersState, market: mersennet::mersennet_orders::MarketId) {
    let book = state.books.get(&market).expect("book exists");

    // 1. Not crossed.
    if let (Some((best_bid, _)), Some((best_ask, _))) =
        (book.bids.iter().next_back(), book.asks.iter().next())
    {
        assert!(
            best_bid < best_ask,
            "crossed book: bid {best_bid} >= ask {best_ask}"
        );
    }

    // 2. Every resting id is a live order at that level, on that side, with size > 0.
    let mut resting = HashSet::new();
    for (side_is_buy, levels) in [(true, &book.bids), (false, &book.asks)] {
        for (price, queue) in levels {
            assert!(!queue.is_empty(), "empty level {price} left on the book");
            for id in queue {
                assert!(
                    resting.insert(*id),
                    "order {id:?} appears twice on the book"
                );
                let o = state
                    .orders
                    .get(id)
                    .unwrap_or_else(|| panic!("resting {id:?} has no order"));
                assert_eq!(o.price, *price, "order {id:?} sits on the wrong level");
                assert_eq!(
                    o.side == Side::Buy,
                    side_is_buy,
                    "order {id:?} on the wrong side"
                );
                assert!(o.size > U256::ZERO, "order {id:?} rests with zero size");
                assert_eq!(o.market, market);
            }
        }
    }

    // 3. Account open-order lists agree with the book.
    let mut listed = HashSet::new();
    for (owner, acct) in &state.accounts {
        for id in &acct.open_orders {
            let o = state
                .orders
                .get(id)
                .unwrap_or_else(|| panic!("open order {id:?} missing"));
            assert_eq!(
                &o.owner, owner,
                "open order {id:?} listed under the wrong account"
            );
            assert!(
                resting.contains(id),
                "account lists {id:?} but it is not on the book"
            );
            listed.insert(*id);
        }
    }
    assert_eq!(
        listed, resting,
        "book and account open-order lists disagree"
    );

    // 4. Positions net to zero.
    let net: i128 = state
        .accounts
        .values()
        .filter_map(|a| a.positions.get(&market).map(|p| p.size))
        .sum();
    assert_eq!(net, 0, "positions do not net to zero");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]

    #[test]
    fn book_invariants_hold_under_random_flow(actions in prop::collection::vec(action(), 1..120)) {
        let mut state = MersennetOrdersState::new();
        let market = state.add_market("PROP/USD", U256::from(1u64), U256::from(1u64));
        for i in 0..4u8 {
            state.deposit_collateral(addr(i), U256::from(1_000_000_000_000u64));
        }
        let mut placed: Vec<OrderId> = Vec::new();

        for a in actions {
            match a {
                Action::Place { owner, buy, price, size, ioc } => {
                    // Rejections (margin, self-trade rules…) are legitimate outcomes;
                    // the invariants must hold either way.
                    if let Ok(outcome) = state.submit_order(
                        addr(owner),
                        market,
                        if buy { Side::Buy } else { Side::Sell },
                        U256::from(price),
                        U256::from(size),
                        if ioc { TimeInForce::Ioc } else { TimeInForce::Gtc },
                    ) {
                        // Fills are conserved: what the taker got is what the trades say.
                        let traded: U256 = outcome.trades.iter().fold(U256::ZERO, |acc, t| acc + t.size);
                        assert_eq!(traded, outcome.filled, "filled amount disagrees with trade sizes");
                        if let Some(id) = outcome.order_id {
                            placed.push(id);
                        }
                    }
                }
                Action::Cancel { nth } => {
                    if !placed.is_empty() {
                        let id = placed[nth % placed.len()];
                        let _ = state.cancel_order(id);
                    }
                }
            }
            check_invariants(&state, market);
        }
    }

    #[test]
    fn ioc_never_rests(price in 90u64..=110, size in 1u64..=50, buy in any::<bool>()) {
        let mut state = MersennetOrdersState::new();
        let market = state.add_market("PROP/USD", U256::from(1u64), U256::from(1u64));
        state.deposit_collateral(addr(0), U256::from(1_000_000_000_000u64));
        let outcome = state.submit_order(
            addr(0), market, if buy { Side::Buy } else { Side::Sell },
            U256::from(price), U256::from(size), TimeInForce::Ioc,
        );
        let book = state.books.get(&market).map(|b| b.bids.len() + b.asks.len()).unwrap_or(0);
        prop_assert_eq!(book, 0, "IOC order rested on an empty book ({:?})", outcome.map(|o| o.order_id));
    }

    #[test]
    fn cancel_removes_exactly_that_order(n in 1usize..12, victim in 0usize..12) {
        let mut state = MersennetOrdersState::new();
        let market = state.add_market("PROP/USD", U256::from(1u64), U256::from(1u64));
        state.deposit_collateral(addr(0), U256::from(1_000_000_000_000u64));
        let ids: Vec<OrderId> = (0..n)
            .map(|i| state.submit_order(addr(0), market, Side::Buy, U256::from(50 + i as u64), U256::from(3u64), TimeInForce::Gtc).expect("resting bid accepted").order_id.expect("GTC bid on an empty book rests"))
            .collect();
        let victim_id = ids[victim % n];
        let before = state.accounts[&addr(0)].open_orders.len();
        let removed = state.cancel_order(victim_id);
        prop_assert!(removed.is_some());
        let acct = &state.accounts[&addr(0)];
        prop_assert_eq!(acct.open_orders.len(), before - 1);
        prop_assert!(!acct.open_orders.contains(&victim_id));
        for other in ids.iter().filter(|i| **i != victim_id) {
            prop_assert!(acct.open_orders.contains(other), "cancel removed an unrelated order");
        }
        check_invariants(&state, market);
    }
}
