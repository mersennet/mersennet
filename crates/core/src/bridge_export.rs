//! Chain-side export path for the Ethereum bridge (Workstream E5).
//!
//! Turns a proven [`BlockProgramOutput`] plus a Groth16-wrapped proof into
//! the exact calldata that `PrimeChainBridge.submitStateProof(uint256[8],
//! uint256[])` expects (see `contracts/src/zk/PrimeChainBridge.sol` and
//! `contracts/src/zk/Groth16Verifier.sol`).
//!
//! The public-input ordering here MUST stay in lockstep with the `PI_*`
//! constants in `PrimeChainBridge.sol` and with
//! [`prime_zkp::sp1::BlockProgramOutput::to_field_elements`]. The bridge
//! compares the root inputs as raw `bytes32` (i.e. `uint256(root)`), so the
//! 32-byte digests are exported verbatim as big-endian EVM words and the
//! `block_number` / `tx_count` counters are left-padded big-endian integers.
//!
//! Generating the actual Groth16-wrapped proof (the bytes consumed by
//! [`decode_groth16_proof`]) and the verifying key installed via
//! `Groth16Verifier.setVerifyingKey` is the remaining out-of-repo step for
//! E5: it requires the SP1 -> Groth16 wrapping circuit and its trusted-setup
//! artifacts. Everything in this module is exercised by unit tests against
//! synthetic vectors so the calldata contract is locked down regardless.

use prime_zkp::sp1::BlockProgramOutput;

/// A 32-byte big-endian EVM word (`uint256` / `bytes32`).
pub type Word = [u8; 32];

/// Number of public inputs the bridge verifier expects (matches
/// `Groth16Verifier.PUBLIC_INPUT_COUNT`).
pub const BRIDGE_PUBLIC_INPUT_COUNT: usize = 9;

/// Length of the 4-byte verifying-key selector prefix SP1 prepends to its
/// on-chain Groth16 proof bytes.
pub const GROTH16_SELECTOR_LEN: usize = 4;

/// Number of 32-byte words in a Groth16 proof (`uint256[8]`: A.x, A.y,
/// B.x[0], B.x[1], B.y[0], B.y[1], C.x, C.y).
pub const GROTH16_PROOF_WORDS: usize = 8;

/// Error returned when a Groth16 proof blob does not match the expected
/// on-chain byte layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeExportError {
    /// The proof blob was too short to hold the selector + 8 words, or had
    /// trailing bytes.
    BadProofLength { got: usize, expected: usize },
}

impl core::fmt::Display for BridgeExportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BridgeExportError::BadProofLength { got, expected } => write!(
                f,
                "groth16 proof blob length {got} != expected {expected} (4-byte selector + 8x32)"
            ),
        }
    }
}

impl std::error::Error for BridgeExportError {}

fn word_from_u64(value: u64) -> Word {
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&value.to_be_bytes());
    word
}

/// Build the bridge public-input vector for a proven block output.
///
/// Ordering matches `PrimeChainBridge.PI_*`:
/// `[prev_state_root, new_state_root, prev_nullifier_root,
/// new_nullifier_root, block_number, block_hash, new_market_state_hash,
/// shielded_event_root, tx_count]`.
pub fn bridge_public_inputs(output: &BlockProgramOutput) -> [Word; BRIDGE_PUBLIC_INPUT_COUNT] {
    [
        output.prev_state_root,
        output.new_state_root,
        output.prev_nullifier_root,
        output.new_nullifier_root,
        word_from_u64(output.block_number),
        output.block_hash,
        output.new_market_state_hash,
        output.shielded_event_root,
        word_from_u64(output.tx_count),
    ]
}

/// Decode an SP1 on-chain Groth16 proof blob (`selector || 8x32 words`) into
/// the `uint256[8]` array the bridge / verifier consumes.
pub fn decode_groth16_proof(proof_bytes: &[u8]) -> Result<[Word; GROTH16_PROOF_WORDS], BridgeExportError> {
    let expected = GROTH16_SELECTOR_LEN + GROTH16_PROOF_WORDS * 32;
    if proof_bytes.len() != expected {
        return Err(BridgeExportError::BadProofLength {
            got: proof_bytes.len(),
            expected,
        });
    }
    let mut words = [[0u8; 32]; GROTH16_PROOF_WORDS];
    for (index, chunk) in proof_bytes[GROTH16_SELECTOR_LEN..]
        .chunks_exact(32)
        .enumerate()
    {
        words[index].copy_from_slice(chunk);
    }
    Ok(words)
}

/// Full `submitStateProof` calldata: the decoded `uint256[8]` proof and the
/// `uint256[]` public inputs, both as big-endian EVM words.
pub struct BridgeSubmission {
    pub proof: [Word; GROTH16_PROOF_WORDS],
    pub public_inputs: [Word; BRIDGE_PUBLIC_INPUT_COUNT],
}

impl BridgeSubmission {
    /// Render the submission as `0x`-prefixed hex words, convenient for an
    /// off-chain relayer building the eth_sendTransaction calldata.
    pub fn to_hex(&self) -> BridgeSubmissionHex {
        BridgeSubmissionHex {
            proof: self.proof.map(|w| format!("0x{}", hex::encode(w))),
            public_inputs: self.public_inputs.map(|w| format!("0x{}", hex::encode(w))),
        }
    }
}

/// Hex-rendered form of [`BridgeSubmission`].
pub struct BridgeSubmissionHex {
    pub proof: [String; GROTH16_PROOF_WORDS],
    pub public_inputs: [String; BRIDGE_PUBLIC_INPUT_COUNT],
}

/// Assemble the full bridge submission from a proven block output and a
/// Groth16-wrapped proof blob.
pub fn build_bridge_submission(
    output: &BlockProgramOutput,
    groth16_proof_bytes: &[u8],
) -> Result<BridgeSubmission, BridgeExportError> {
    Ok(BridgeSubmission {
        proof: decode_groth16_proof(groth16_proof_bytes)?,
        public_inputs: bridge_public_inputs(output),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_output() -> BlockProgramOutput {
        BlockProgramOutput {
            prev_state_root: [0x11; 32],
            new_state_root: [0x22; 32],
            prev_nullifier_root: [0x33; 32],
            new_nullifier_root: [0x44; 32],
            block_number: 7,
            block_hash: [0x55; 32],
            new_market_state_hash: [0x66; 32],
            shielded_event_root: [0x77; 32],
            tx_count: 3,
        }
    }

    #[test]
    fn public_inputs_match_bridge_ordering() {
        let output = sample_output();
        let inputs = bridge_public_inputs(&output);
        assert_eq!(inputs.len(), BRIDGE_PUBLIC_INPUT_COUNT);
        // PI_PREV_STATE_ROOT .. PI_SHIELDED_EVENT_ROOT are verbatim digests.
        assert_eq!(inputs[0], [0x11; 32]);
        assert_eq!(inputs[1], [0x22; 32]);
        assert_eq!(inputs[2], [0x33; 32]);
        assert_eq!(inputs[3], [0x44; 32]);
        assert_eq!(inputs[5], [0x55; 32]);
        assert_eq!(inputs[6], [0x66; 32]);
        assert_eq!(inputs[7], [0x77; 32]);
        // PI_BLOCK_NUMBER / PI_TX_COUNT are big-endian counters.
        assert_eq!(inputs[4], word_from_u64(7));
        assert_eq!(inputs[8], word_from_u64(3));
        assert_eq!(inputs[4][31], 7);
        assert_eq!(inputs[8][31], 3);
    }

    #[test]
    fn decode_groth16_proof_strips_selector() {
        let mut blob = vec![0xde, 0xad, 0xbe, 0xef]; // 4-byte vkey selector
        for word in 0u8..8 {
            blob.extend_from_slice(&[word + 1; 32]);
        }
        let proof = decode_groth16_proof(&blob).unwrap();
        for (word, value) in proof.iter().enumerate() {
            assert_eq!(*value, [(word as u8) + 1; 32]);
        }
    }

    #[test]
    fn decode_groth16_proof_rejects_bad_length() {
        let err = decode_groth16_proof(&[0u8; 10]).unwrap_err();
        assert_eq!(
            err,
            BridgeExportError::BadProofLength {
                got: 10,
                expected: 4 + 8 * 32
            }
        );
    }

    #[test]
    fn build_submission_round_trips_to_hex() {
        let output = sample_output();
        let mut blob = vec![0x00, 0x47, 0xc7, 0xa7];
        blob.extend_from_slice(&[0xab; 32 * 8]);
        let submission = build_bridge_submission(&output, &blob).unwrap();
        let hex = submission.to_hex();
        assert_eq!(hex.public_inputs[0], format!("0x{}", "11".repeat(32)));
        assert_eq!(hex.proof[0], format!("0x{}", "ab".repeat(32)));
        assert_eq!(hex.public_inputs.len(), BRIDGE_PUBLIC_INPUT_COUNT);
    }
}
