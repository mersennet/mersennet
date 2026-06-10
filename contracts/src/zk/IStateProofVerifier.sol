// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IStateProofVerifier
/// @notice Verifies a Mersennet block state-transition proof on Ethereum.
/// @dev The proof is a Groth16 proof over BN254 wrapping the SP1
///      state-transition program. The public inputs are the nine field
///      elements committed by `BlockProgramOutput::to_field_elements`
///      (see `crates/zkp/src/sp1.rs`), in the exact same order:
///
///      0. prev_state_root
///      1. new_state_root
///      2. prev_nullifier_root
///      3. new_nullifier_root
///      4. block_number
///      5. block_hash
///      6. new_market_state_hash
///      7. shielded_event_root
///      8. tx_count
interface IStateProofVerifier {
    /// @notice Number of public inputs expected by the verifier.
    function publicInputCount() external pure returns (uint256);

    /// @notice Verify a Groth16 proof against the committed public inputs.
    /// @param proof   The Groth16 proof, encoded as [A.x, A.y, B.x0, B.x1, B.y0, B.y1, C.x, C.y].
    /// @param input   The public inputs (see ordering above). Length must equal `publicInputCount()`.
    /// @return ok     True iff the proof verifies.
    function verifyProof(uint256[8] calldata proof, uint256[] calldata input)
        external
        view
        returns (bool ok);
}
