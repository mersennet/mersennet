// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../../src/zk/PrimeChainBridge.sol";
import "../../src/zk/IStateProofVerifier.sol";

/// @dev Deterministic stand-in for the Groth16 verifier so the bridge logic
///      can be tested without a real proving key. `result` toggles outcomes.
contract MockStateProofVerifier is IStateProofVerifier {
    bool public result = true;

    function setResult(bool r) external {
        result = r;
    }

    function publicInputCount() external pure returns (uint256) {
        return 9;
    }

    function verifyProof(uint256[8] calldata, uint256[] calldata) external view returns (bool) {
        return result;
    }
}

contract PrimeChainBridgeTest is Test {
    MockStateProofVerifier internal verifier;
    PrimeChainBridge internal bridge;

    bytes32 internal constant GENESIS_STATE = bytes32(uint256(0xa11ce));
    bytes32 internal constant GENESIS_NULL = bytes32(uint256(0xb0b));
    bytes32 internal constant VKEY = bytes32(uint256(0x0047c7a7));

    address internal alice = address(0xA11CE);
    address internal bob = address(0xB0B);

    function setUp() public {
        verifier = new MockStateProofVerifier();
        bridge = new PrimeChainBridge(verifier, VKEY, GENESIS_STATE, GENESIS_NULL);
    }

    // Build a full 9-element public-input vector.
    function _input(
        bytes32 prevState,
        bytes32 newState,
        bytes32 prevNull,
        bytes32 newNull,
        uint64 blockNumber
    ) internal pure returns (uint256[] memory input) {
        input = new uint256[](9);
        input[0] = uint256(prevState);
        input[1] = uint256(newState);
        input[2] = uint256(prevNull);
        input[3] = uint256(newNull);
        input[4] = blockNumber;
        input[5] = 0; // block_hash
        input[6] = 0; // market_state_hash
        input[7] = uint256(0xe7e7); // shielded_event_root
        input[8] = 3; // tx_count
    }

    function _emptyProof() internal pure returns (uint256[8] memory p) {
        return p;
    }

    // --- deposits ---

    function test_DepositLocksEthAndEmits() public {
        vm.deal(alice, 10 ether);
        vm.prank(alice);
        bridge.deposit{value: 1 ether}(bytes32(uint256(0xfeed)));

        assertEq(bridge.totalLocked(), 1 ether);
        assertEq(bridge.depositNonce(), 1);
        assertEq(address(bridge).balance, 1 ether);
    }

    function test_DepositRevertsWhenZero() public {
        vm.expectRevert(PrimeChainBridge.ZeroDeposit.selector);
        bridge.deposit{value: 0}(bytes32(0));
    }

    function test_DepositRevertsWhenPaused() public {
        bridge.setDepositsPaused(true);
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        vm.expectRevert(PrimeChainBridge.DepositsArePaused.selector);
        bridge.deposit{value: 1 ether}(bytes32(0));
    }

    function test_OnlyOwnerCanPause() public {
        vm.prank(alice);
        vm.expectRevert(PrimeChainBridge.NotOwner.selector);
        bridge.setDepositsPaused(true);
    }

    // --- state proof intake ---

    function test_SubmitStateProofAdvancesRoots() public {
        bytes32 newState = bytes32(uint256(0xc0ffee));
        bytes32 newNull = bytes32(uint256(0xdecaf));
        uint256[] memory input = _input(GENESIS_STATE, newState, GENESIS_NULL, newNull, 1);

        bridge.submitStateProof(_emptyProof(), input);

        assertEq(bridge.latestProvenBlock(), 1);
        assertEq(bridge.shieldedStateRoot(), newState);
        assertEq(bridge.nullifierRoot(), newNull);
    }

    function test_SubmitStateProofRejectsBadProof() public {
        verifier.setResult(false);
        uint256[] memory input =
            _input(GENESIS_STATE, bytes32(uint256(1)), GENESIS_NULL, bytes32(uint256(2)), 1);
        vm.expectRevert(PrimeChainBridge.ProofRejected.selector);
        bridge.submitStateProof(_emptyProof(), input);
    }

    function test_SubmitStateProofRejectsDiscontinuity() public {
        uint256[] memory input =
            _input(bytes32(uint256(0xdead)), bytes32(uint256(1)), GENESIS_NULL, bytes32(uint256(2)), 1);
        vm.expectRevert(PrimeChainBridge.StateDiscontinuity.selector);
        bridge.submitStateProof(_emptyProof(), input);
    }

    function test_SubmitStateProofRejectsNonMonotonic() public {
        // First proof advances to block 5.
        bytes32 s1 = bytes32(uint256(0x111));
        bytes32 n1 = bytes32(uint256(0x222));
        bridge.submitStateProof(_emptyProof(), _input(GENESIS_STATE, s1, GENESIS_NULL, n1, 5));

        // Replaying block 5 (or lower) must revert.
        vm.expectRevert(PrimeChainBridge.NonMonotonicBlock.selector);
        bridge.submitStateProof(_emptyProof(), _input(s1, bytes32(uint256(0x333)), n1, bytes32(uint256(0x444)), 5));
    }

    function test_SubmitStateProofChainsAcrossBlocks() public {
        bytes32 s1 = bytes32(uint256(0x111));
        bytes32 n1 = bytes32(uint256(0x222));
        bridge.submitStateProof(_emptyProof(), _input(GENESIS_STATE, s1, GENESIS_NULL, n1, 1));

        bytes32 s2 = bytes32(uint256(0x333));
        bytes32 n2 = bytes32(uint256(0x444));
        bridge.submitStateProof(_emptyProof(), _input(s1, s2, n1, n2, 2));

        assertEq(bridge.latestProvenBlock(), 2);
        assertEq(bridge.shieldedStateRoot(), s2);
        assertEq(bridge.nullifierRoot(), n2);
    }

    function test_SubmitStateProofRejectsBadInputLength() public {
        uint256[] memory bad = new uint256[](3);
        vm.expectRevert(PrimeChainBridge.BadPublicInputs.selector);
        bridge.submitStateProof(_emptyProof(), bad);
    }

    // --- withdrawals ---

    function _hashPair(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        return a <= b
            ? keccak256(abi.encodePacked(a, b))
            : keccak256(abi.encodePacked(b, a));
    }

    function test_WithdrawWithValidMerkleProof() public {
        // Fund the bridge.
        vm.deal(alice, 10 ether);
        vm.prank(alice);
        bridge.deposit{value: 5 ether}(bytes32(uint256(0x1)));

        // Build a 2-leaf withdrawal tree: leaf for bob + a sibling.
        uint256 amount = 2 ether;
        uint256 leafNonce = 42;
        bytes32 leaf = keccak256(abi.encode(bob, amount, leafNonce));
        bytes32 sibling = keccak256("sibling");
        bytes32 root = _hashPair(leaf, sibling);

        // Advance proven state so shieldedStateRoot == withdrawal tree root.
        bridge.submitStateProof(
            _emptyProof(), _input(GENESIS_STATE, root, GENESIS_NULL, bytes32(uint256(7)), 1)
        );

        bytes32[] memory proof = new bytes32[](1);
        proof[0] = sibling;

        uint256 bobBefore = bob.balance;
        bridge.withdraw(bob, amount, leafNonce, proof);

        assertEq(bob.balance, bobBefore + amount);
        assertEq(bridge.totalLocked(), 3 ether);
        assertTrue(bridge.withdrawalSpent(leaf));
    }

    function test_WithdrawRejectsReplay() public {
        vm.deal(alice, 10 ether);
        vm.prank(alice);
        bridge.deposit{value: 5 ether}(bytes32(uint256(0x1)));

        uint256 amount = 1 ether;
        uint256 leafNonce = 7;
        bytes32 leaf = keccak256(abi.encode(bob, amount, leafNonce));
        bytes32 sibling = keccak256("s2");
        bytes32 root = _hashPair(leaf, sibling);
        bridge.submitStateProof(
            _emptyProof(), _input(GENESIS_STATE, root, GENESIS_NULL, bytes32(uint256(7)), 1)
        );

        bytes32[] memory proof = new bytes32[](1);
        proof[0] = sibling;
        bridge.withdraw(bob, amount, leafNonce, proof);

        vm.expectRevert(PrimeChainBridge.AlreadyWithdrawn.selector);
        bridge.withdraw(bob, amount, leafNonce, proof);
    }

    function test_WithdrawRejectsBadProof() public {
        vm.deal(alice, 10 ether);
        vm.prank(alice);
        bridge.deposit{value: 5 ether}(bytes32(uint256(0x1)));
        bridge.submitStateProof(
            _emptyProof(), _input(GENESIS_STATE, bytes32(uint256(0x999)), GENESIS_NULL, bytes32(uint256(7)), 1)
        );

        bytes32[] memory proof = new bytes32[](1);
        proof[0] = keccak256("wrong");
        vm.expectRevert(PrimeChainBridge.InvalidMerkleProof.selector);
        bridge.withdraw(bob, 1 ether, 1, proof);
    }

    function test_WithdrawRejectsBeforeAnyProof() public {
        bytes32[] memory proof = new bytes32[](0);
        vm.expectRevert(PrimeChainBridge.NotYetProven.selector);
        bridge.withdraw(bob, 1 ether, 1, proof);
    }
}
