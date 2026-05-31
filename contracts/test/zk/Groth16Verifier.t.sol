// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../../src/zk/Groth16Verifier.sol";

contract Groth16VerifierTest is Test {
    Groth16Verifier internal verifier;

    // BN254 G1 generator.
    uint256 internal constant G1X = 1;
    uint256 internal constant G1Y = 2;
    // BN254 scalar field modulus.
    uint256 internal constant R =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;

    function setUp() public {
        verifier = new Groth16Verifier();
    }

    function _g1() internal pure returns (Groth16Verifier.G1Point memory) {
        return Groth16Verifier.G1Point(G1X, G1Y);
    }

    // BN254 G2 generator, in precompile encoding.
    function _g2() internal pure returns (Groth16Verifier.G2Point memory) {
        return Groth16Verifier.G2Point(
            [
                11559732032986387107991004021392285783925812861821192530917403151452391805634,
                10857046999023057135944570762232829481370756359578518086990519993285655852781
            ],
            [
                4082367875863433681332203403145435568316851327593401208105741076214120093531,
                8495653923123431417604973247489272438418190587263600148770280649306958101930
            ]
        );
    }

    function _installGeneratorVK() internal {
        Groth16Verifier.G1Point[10] memory ic;
        for (uint256 i = 0; i < 10; i++) {
            ic[i] = _g1();
        }
        verifier.setVerifyingKey(_g1(), _g2(), _g2(), _g2(), ic);
    }

    function test_PublicInputCountIsNine() public view {
        assertEq(verifier.publicInputCount(), 9);
    }

    function test_VerifyRevertsBeforeConfigured() public {
        uint256[8] memory proof;
        uint256[] memory input = new uint256[](9);
        vm.expectRevert(Groth16Verifier.NotConfigured.selector);
        verifier.verifyProof(proof, input);
    }

    function test_SetVerifyingKeyOnlyOwner() public {
        Groth16Verifier.G1Point[10] memory ic;
        for (uint256 i = 0; i < 10; i++) {
            ic[i] = _g1();
        }
        vm.prank(address(0xBEEF));
        vm.expectRevert(Groth16Verifier.NotOwner.selector);
        verifier.setVerifyingKey(_g1(), _g2(), _g2(), _g2(), ic);
    }

    function test_ConfigureAndLock() public {
        assertFalse(verifier.isConfigured());
        _installGeneratorVK();
        assertTrue(verifier.isConfigured());
        verifier.lockVerifyingKey();
        assertTrue(verifier.vkLocked());

        Groth16Verifier.G1Point[10] memory ic;
        for (uint256 i = 0; i < 10; i++) {
            ic[i] = _g1();
        }
        vm.expectRevert(Groth16Verifier.AlreadyLocked.selector);
        verifier.setVerifyingKey(_g1(), _g2(), _g2(), _g2(), ic);
    }

    function test_VerifyRejectsBadInputLength() public {
        _installGeneratorVK();
        uint256[8] memory proof;
        uint256[] memory input = new uint256[](8);
        vm.expectRevert(Groth16Verifier.BadInputLength.selector);
        verifier.verifyProof(proof, input);
    }

    function test_VerifyRejectsInputOutOfField() public {
        _installGeneratorVK();
        uint256[8] memory proof = [G1X, G1Y, uint256(0), uint256(0), uint256(0), uint256(0), G1X, G1Y];
        uint256[] memory input = new uint256[](9);
        input[0] = R; // exactly the modulus -> out of field
        vm.expectRevert(Groth16Verifier.InputOutOfField.selector);
        verifier.verifyProof(proof, input);
    }

    /// @dev With the generator-only VK and generator proof points, the pairing
    ///      executes on-chain and returns false (not a valid proof), proving
    ///      the precompile plumbing works end-to-end without reverting.
    function test_VerifyPlumbingReturnsFalseForNonProof() public {
        _installGeneratorVK();
        Groth16Verifier.G2Point memory g2 = _g2();
        uint256[8] memory proof = [
            G1X,
            G1Y,
            g2.x[0],
            g2.x[1],
            g2.y[0],
            g2.y[1],
            G1X,
            G1Y
        ];
        uint256[] memory input = new uint256[](9);
        for (uint256 i = 0; i < 9; i++) {
            input[i] = i + 1;
        }
        assertFalse(verifier.verifyProof(proof, input));
    }
}
