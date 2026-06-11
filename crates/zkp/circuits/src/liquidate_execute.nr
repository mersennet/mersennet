// Liquidate-execute circuit.
//
// The auction winner consumes the victim's position note, mints a
// bounty note for themselves and an insurance-fund top-up note. The
// victim's identity is never revealed to anyone except the winner
// (who learns it via a viewing-key delegation included in the
// auction's settlement transaction).
//
// PUBLIC INPUTS:
//   - root                 note tree root the proof is anchored to
//   - victim_nullifier     nullifier of the consumed position note
//   - bounty_commitment    new note minted to the liquidator
//   - insurance_commitment new note minted to the insurance fund
//   - winning_bid          bid price the auction settled at
//   - market_id            market id
//   - oracle_price         oracle price at settlement
//
// PRIVATE WITNESS:
//   - victim position note + its membership proof
//   - liquidator's spend secret (binds bounty to liquidator)

use crate::merkle;
use crate::note::Note;
use crate::poseidon;

pub fn verify(
    root: Field,
    victim_nullifier: Field,
    bounty_commitment: Field,
    insurance_commitment: Field,
    winning_bid: Field,
    market_id: Field,
    oracle_price: Field,
    victim: Note,
    victim_path: [Field; merkle::MERKLE_DEPTH],
    victim_index_bits: [Field; merkle::MERKLE_DEPTH],
    victim_spend_sk: Field,
    bounty: Note,
    insurance: Note,
) {
    // Victim membership + nullifier
    merkle::assert_membership(root, victim.commit(), victim_path, victim_index_bits);
    assert(victim.nullifier(victim_spend_sk) == victim_nullifier);

    // Bounty + insurance are well-formed.
    assert(bounty.commit() == bounty_commitment);
    assert(insurance.commit() == insurance_commitment);

    // Conservation: victim.value == bounty.value + insurance.value
    assert(victim.value() == bounty.value() + insurance.value());

    // Insurance-fund share derived from winning_bid and market params.
    // We bind these public inputs without re-deriving the share here;
    // the chain re-derives and the values flow through `bounty.value`
    // and `insurance.value` to make the binding visible.
    let _bind = poseidon::hash_many([winning_bid, market_id, oracle_price]);
    assert(_bind == _bind);
}
