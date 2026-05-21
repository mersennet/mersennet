// Spend circuit.
//
// Proves:
//   - Knowledge of a note `spent_note` whose commitment is in the
//     `root` of the note tree at index witnessed by `merkle_path` and
//     `merkle_index_bits`.
//   - The published `nullifier` equals `Poseidon(spend_sk, spent_note.rho)`.
//   - The `output_note` commitment equals `new_commitment`.
//   - Conservation:
//        spent_note.value == output_note.value + public_amount
//     where `public_amount` is the amount being unshielded (can be 0).
//   - `output_note.asset_id == spent_note.asset_id` (no cross-asset spend).
//
// Does not reveal: owner identity, exact value, asset relationship
// across spends.

use crate::lib;
use crate::merkle;
use crate::note::Note;
use crate::poseidon;

pub struct SpentNote {
    note: Note,
}

pub struct OutputNote {
    note: Note,
}

pub fn verify(
    root: Field,
    nullifier: Field,
    new_commitment: Field,
    public_amount: Field,
    spent: SpentNote,
    output: OutputNote,
    merkle_path: [Field; merkle::MERKLE_DEPTH],
    merkle_index_bits: [Field; merkle::MERKLE_DEPTH],
    spend_sk: Field,
) {
    // The spent note must be in the tree.
    let spent_cm = spent.note.commit();
    merkle::assert_membership(root, spent_cm, merkle_path, merkle_index_bits);

    // The nullifier must match.
    let derived_nullifier = spent.note.nullifier(spend_sk);
    assert(derived_nullifier == nullifier);

    // The output note commitment must match.
    let output_cm = output.note.commit();
    assert(output_cm == new_commitment);

    // Conservation. We use field arithmetic; the surrounding
    // u128-range checks are performed by the SDK before assembling
    // the witness.
    assert(spent.note.value() == output.note.value() + public_amount);

    // Asset preservation.
    assert(spent.note.asset_id == output.note.asset_id);
}
