// Entry point for `nargo check`. Real circuits each have their own
// `Nargo.toml` package under `crates/zkp/circuits/<circuit>/` once we
// move to per-circuit packages (Phase 1.4). For now, this single
// entry point exposes the `spend` circuit and the others sit in
// sibling `.nr` files that the library re-exports.

mod lib;
mod spend;
mod output;
mod join_split;
mod order_place;
mod liquidate_claim;
mod liquidate_execute;

use crate::spend;

// `main` is the program input the chain verifies on every spend-only
// transaction. For multi-circuit transactions we wrap them in
// `join_split` instead.
//
// PUBLIC INPUTS (in order):
//   - root              note tree root the spend is anchored to
//   - nullifier         nullifier published by this spend
//   - new_commitment    commitment of the change note (0 if none)
//   - public_amount     amount being unshielded (0 if pure shielded transfer)
fn main(
    // Public
    root: pub Field,
    nullifier: pub Field,
    new_commitment: pub Field,
    public_amount: pub Field,
    // Private witness
    spent_note: spend::SpentNote,
    output_note: spend::OutputNote,
    merkle_path: [Field; 32],
    merkle_index_bits: [Field; 32],
    spend_sk: Field,
) {
    spend::verify(
        root,
        nullifier,
        new_commitment,
        public_amount,
        spent_note,
        output_note,
        merkle_path,
        merkle_index_bits,
        spend_sk,
    );
}
