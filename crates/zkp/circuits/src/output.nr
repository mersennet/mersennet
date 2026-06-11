// Output circuit.
//
// Proves that a note commitment was constructed from a well-formed
// note with `value <= MAX_VALUE` and a known recipient. Used to
// **shield** transparent funds into the pool.
//
// PUBLIC INPUTS:
//   - commitment        commitment of the freshly-minted note
//   - asset_id          which asset (for the bridge contract)
//   - public_amount     amount being shielded (debited from transparent)
//
// PRIVATE WITNESS:
//   - the note's value, owner_pk, rho, psi.
//
// Conservation: `note.value == public_amount`. (Shielding is 1:1.)

use crate::note::Note;

global MAX_VALUE: Field = 0xffffffffffffffffffffffffffffffff; // 2^128 - 1

pub fn verify(
    commitment: Field,
    asset_id: Field,
    public_amount: Field,
    note: Note,
) {
    assert(note.asset_id == asset_id);
    let cm = note.commit();
    assert(cm == commitment);
    assert(note.value() == public_amount);
    // value <= MAX_VALUE - Noir's u128 type handles this in the host
    // witness assembly; the field-level check is a soft bound here.
    assert(note.value().lt(MAX_VALUE + 1));
}
