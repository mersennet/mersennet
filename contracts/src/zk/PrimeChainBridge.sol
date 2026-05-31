// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IStateProofVerifier.sol";

/// @title PrimeChainBridge
/// @notice Ethereum-side anchor for Prime Chain's privacy fork. It consumes
///         Groth16-wrapped SP1 state-transition proofs to advance a canonical
///         view of the shielded state root, and operates a deposit / withdraw
///         message bus secured by that proven state.
/// @dev Public inputs follow `BlockProgramOutput::to_field_elements`
///      (crates/zkp/src/sp1.rs). Withdrawal authorizations are proven by
///      Merkle inclusion against the latest proven `shieldedStateRoot`; the
///      leaf encoding below is the integration contract the chain-side
///      unshield-authorization tree must match.
contract PrimeChainBridge {
    // --- public-input indices (must match to_field_elements ordering) ---
    uint256 internal constant PI_PREV_STATE_ROOT = 0;
    uint256 internal constant PI_NEW_STATE_ROOT = 1;
    uint256 internal constant PI_PREV_NULLIFIER_ROOT = 2;
    uint256 internal constant PI_NEW_NULLIFIER_ROOT = 3;
    uint256 internal constant PI_BLOCK_NUMBER = 4;
    uint256 internal constant PI_BLOCK_HASH = 5;
    uint256 internal constant PI_MARKET_STATE_HASH = 6;
    uint256 internal constant PI_SHIELDED_EVENT_ROOT = 7;
    uint256 internal constant PI_TX_COUNT = 8;

    IStateProofVerifier public immutable verifier;
    /// @notice Pinned SP1 program verifying-key hash (PRIME_SP1_VKEY_HASH).
    bytes32 public immutable programVKeyHash;

    address public owner;
    bool public depositsPaused;

    // --- proven canonical state ---
    uint64 public latestProvenBlock;
    bytes32 public shieldedStateRoot;
    bytes32 public nullifierRoot;

    // --- message bus accounting ---
    uint256 public depositNonce;
    uint256 public totalLocked;
    /// @notice Spent withdrawal leaves (nullifies replay).
    mapping(bytes32 => bool) public withdrawalSpent;

    event StateProofAccepted(
        uint64 indexed blockNumber,
        bytes32 prevStateRoot,
        bytes32 newStateRoot,
        bytes32 newNullifierRoot,
        bytes32 shieldedEventRoot
    );
    event DepositLocked(
        uint256 indexed nonce,
        bytes32 indexed shieldedRecipient,
        address indexed from,
        uint256 amount
    );
    event Withdrawn(
        bytes32 indexed leaf,
        address indexed recipient,
        uint256 amount,
        uint256 leafNonce
    );
    event DepositsPaused(bool paused);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    error NotOwner();
    error DepositsArePaused();
    error ZeroDeposit();
    error BadPublicInputs();
    error NonMonotonicBlock();
    error StateDiscontinuity();
    error ProofRejected();
    error NotYetProven();
    error AlreadyWithdrawn();
    error InvalidMerkleProof();
    error TransferFailed();
    error Reentrancy();

    uint256 private _lock = 1;

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier nonReentrant() {
        if (_lock != 1) revert Reentrancy();
        _lock = 2;
        _;
        _lock = 1;
    }

    /// @param verifier_         The Groth16 state-proof verifier.
    /// @param programVKeyHash_  Pinned SP1 program vkey hash (for off-chain audit binding).
    /// @param genesisStateRoot  The shielded state root at the activation block.
    /// @param genesisNullifierRoot The nullifier root at the activation block.
    constructor(
        IStateProofVerifier verifier_,
        bytes32 programVKeyHash_,
        bytes32 genesisStateRoot,
        bytes32 genesisNullifierRoot
    ) {
        verifier = verifier_;
        programVKeyHash = programVKeyHash_;
        shieldedStateRoot = genesisStateRoot;
        nullifierRoot = genesisNullifierRoot;
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    function transferOwnership(address newOwner) external onlyOwner {
        emit OwnershipTransferred(owner, newOwner);
        owner = newOwner;
    }

    function setDepositsPaused(bool paused) external onlyOwner {
        depositsPaused = paused;
        emit DepositsPaused(paused);
    }

    /// @notice Submit a Groth16-wrapped state-transition proof to advance the
    ///         canonical shielded roots. Enforces block monotonicity and
    ///         prev->new root continuity so the on-chain view tracks a single
    ///         canonical chain.
    function submitStateProof(uint256[8] calldata proof, uint256[] calldata input) external {
        if (input.length != verifier.publicInputCount()) revert BadPublicInputs();

        uint64 blockNumber = uint64(input[PI_BLOCK_NUMBER]);
        if (blockNumber <= latestProvenBlock && latestProvenBlock != 0) revert NonMonotonicBlock();
        if (blockNumber == 0) revert NonMonotonicBlock();

        // Continuity: the proof's prev roots must match our current view.
        if (
            bytes32(input[PI_PREV_STATE_ROOT]) != shieldedStateRoot
                || bytes32(input[PI_PREV_NULLIFIER_ROOT]) != nullifierRoot
        ) {
            revert StateDiscontinuity();
        }

        if (!verifier.verifyProof(proof, input)) revert ProofRejected();

        shieldedStateRoot = bytes32(input[PI_NEW_STATE_ROOT]);
        nullifierRoot = bytes32(input[PI_NEW_NULLIFIER_ROOT]);
        latestProvenBlock = blockNumber;

        emit StateProofAccepted(
            blockNumber,
            bytes32(input[PI_PREV_STATE_ROOT]),
            shieldedStateRoot,
            nullifierRoot,
            bytes32(input[PI_SHIELDED_EVENT_ROOT])
        );
    }

    /// @notice Lock ETH on Ethereum and request a shielded note on Prime Chain.
    /// @param shieldedRecipient The shielded identity commitment to credit.
    function deposit(bytes32 shieldedRecipient) external payable {
        if (depositsPaused) revert DepositsArePaused();
        if (msg.value == 0) revert ZeroDeposit();
        uint256 nonce = depositNonce++;
        totalLocked += msg.value;
        emit DepositLocked(nonce, shieldedRecipient, msg.sender, msg.value);
    }

    /// @notice Withdraw ETH that was authorized (unshielded) on Prime Chain.
    /// @dev The withdrawal leaf is `keccak256(abi.encode(recipient, amount, leafNonce))`
    ///      and must be included in the latest proven `shieldedStateRoot` via a
    ///      sorted-pair keccak Merkle tree. Each leaf can be spent once.
    function withdraw(
        address recipient,
        uint256 amount,
        uint256 leafNonce,
        bytes32[] calldata merkleProof
    ) external nonReentrant {
        if (latestProvenBlock == 0) revert NotYetProven();
        bytes32 leaf = keccak256(abi.encode(recipient, amount, leafNonce));
        if (withdrawalSpent[leaf]) revert AlreadyWithdrawn();
        if (!_verifyMerkle(merkleProof, shieldedStateRoot, leaf)) revert InvalidMerkleProof();

        withdrawalSpent[leaf] = true;
        totalLocked -= amount;

        (bool ok,) = recipient.call{value: amount}("");
        if (!ok) revert TransferFailed();

        emit Withdrawn(leaf, recipient, amount, leafNonce);
    }

    /// @dev Standard sorted-pair keccak Merkle inclusion check.
    function _verifyMerkle(bytes32[] calldata proof, bytes32 root, bytes32 leaf)
        internal
        pure
        returns (bool)
    {
        bytes32 computed = leaf;
        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 sibling = proof[i];
            if (computed <= sibling) {
                computed = keccak256(abi.encodePacked(computed, sibling));
            } else {
                computed = keccak256(abi.encodePacked(sibling, computed));
            }
        }
        return computed == root;
    }
}
