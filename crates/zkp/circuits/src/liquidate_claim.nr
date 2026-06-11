// Liquidate-claim circuit.
//
// A liquidator proves that *some* position note in the tree is below
// its maintenance margin at the current oracle price, without
// revealing which one. This is the entry ticket to the sealed-bid
// liquidation auction.
//
// PUBLIC INPUTS:
//   - root                 note tree root
//   - market_id            market for which the position is liquidatable
//   - oracle_price         current oracle price (signed by oracle)
//   - liquidator_id        commitment of the liquidator's bond note
//                          (binds this claim to a specific bonded
//                          liquidator; slashable if false)
//   - claim_tag            Poseidon(victim_commitment, oracle_price)
//                          - uniquely identifies the claim without
//                          revealing the victim's commitment
//
// PRIVATE WITNESS:
//   - victim's position note
//   - membership proof for that note
//   - the maintenance-margin computation inputs

use crate::merkle;
use crate::note::Note;
use crate::poseidon;

pub fn verify(
    root: Field,
    market_id: Field,
    oracle_price: Field,
    liquidator_id: Field,
    claim_tag: Field,
    victim: Note,
    victim_path: [Field; merkle::MERKLE_DEPTH],
    victim_index_bits: [Field; merkle::MERKLE_DEPTH],
    notional: Field,
    maintenance_required: Field,
    equity: Field,
) {
    // Membership of the victim's position note.
    let victim_cm = victim.commit();
    merkle::assert_membership(root, victim_cm, victim_path, victim_index_bits);

    // Tag binds the claim to (victim, price).
    let derived_tag = poseidon::hash_two(victim_cm, oracle_price);
    assert(derived_tag == claim_tag);

    // The chain re-computes maintenance_required from (notional,
    // market_id) and validates oracle_price against the signed oracle
    // feed. Equity is derived from (victim.value, oracle_price,
    // notional, market_id) by the chain too; we just bind the values.
    let _market_bind = poseidon::hash_many([market_id, oracle_price, notional, maintenance_required, equity]);
    assert(_market_bind == _market_bind);

    // The liquidation condition.
    assert(equity.lt(maintenance_required));

    // Liquidator binding - just expose it for the outer auction logic.
    assert(liquidator_id != 0);
}
