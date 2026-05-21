// Join-split circuit.
//
// Generalisation of `spend` with 2 inputs and 2 outputs, plus an
// optional `public_amount` that goes in/out of the shielded pool.
// This is the workhorse transaction for everyday transfers.
//
// PUBLIC INPUTS:
//   - root                anchor root the spends are taken against
//   - nullifier1          nullifier of input 1
//   - nullifier2          nullifier of input 2 (0 if unused)
//   - commitment1         commitment of output 1
//   - commitment2         commitment of output 2 (0 if unused)
//   - public_amount       net amount entering (positive) or leaving
//                         (encoded as negative via field wrap-around)
//                         the shielded pool. The bridge contract is
//                         the only entity that sees this and binds
//                         it to a transparent ERC20 transfer.

use crate::lib;
use crate::merkle;
use crate::note::Note;
use crate::poseidon;

pub fn verify(
    // Public
    root: Field,
    nullifier1: Field,
    nullifier2: Field,
    commitment1: Field,
    commitment2: Field,
    public_amount: Field,
    // Private witness
    input1: Note,
    input2: Note,
    input1_path: [Field; merkle::MERKLE_DEPTH],
    input1_index_bits: [Field; merkle::MERKLE_DEPTH],
    input2_path: [Field; merkle::MERKLE_DEPTH],
    input2_index_bits: [Field; merkle::MERKLE_DEPTH],
    output1: Note,
    output2: Note,
    spend_sk: Field,
    input2_used: Field,
    output2_used: Field,
) {
    // Bool checks
    assert(input2_used * (input2_used - 1) == 0);
    assert(output2_used * (output2_used - 1) == 0);

    // --- Input 1: always used.
    merkle::assert_membership(root, input1.commit(), input1_path, input1_index_bits);
    assert(input1.nullifier(spend_sk) == nullifier1);

    // --- Input 2: optionally used.
    let used_in2 = input2_used;
    let n2 = input2.nullifier(spend_sk);
    // If input2_used == 0, nullifier2 must equal 0.
    assert(nullifier2 == n2 * used_in2);
    // Membership only enforced when used. We multiplex by mixing the
    // claimed root with `root` when used and with `input1.commit()`
    // (which trivially holds) when not used. Cleaner: compute
    // membership and gate the equality.
    if input2_used == 1 {
        merkle::assert_membership(root, input2.commit(), input2_path, input2_index_bits);
    }

    // --- Output 1: always used.
    assert(output1.commit() == commitment1);

    // --- Output 2: optionally used.
    let used_out2 = output2_used;
    assert(commitment2 == output2.commit() * used_out2);

    // --- Conservation.
    // total_in + public_in == total_out
    let total_in = input1.value() + input2.value() * used_in2;
    let total_out = output1.value() + output2.value() * used_out2;
    assert(total_in + public_amount == total_out);

    // --- Asset preservation. Mixed-asset join-splits go through the
    // dedicated order_place circuit, not this one.
    assert(input1.asset_id == output1.asset_id);
    if input2_used == 1 {
        assert(input2.asset_id == input1.asset_id);
    }
    if output2_used == 1 {
        assert(output2.asset_id == input1.asset_id);
    }

    // Forbid the trivial nullifier1 == nullifier2 case unless input2
    // is unused. (Duplicate nullifiers in a single tx would already be
    // rejected by the chain's nullifier-set logic, but we add a
    // belt-and-braces constraint here.)
    if input2_used == 1 {
        assert(nullifier1 != nullifier2);
    }
}
