// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IStateProofVerifier.sol";

/// @title Groth16Verifier
/// @notice On-chain Groth16 verifier over the BN254 (alt_bn128) curve for
///         Prime Chain state-transition proofs.
/// @dev Uses the EVM precompiles ecAdd (0x06), ecMul (0x07) and ecPairing
///      (0x08). The verifying key is set once by the deployer after the SP1
///      Groth16 wrapping circuit is finalized, then permanently locked. This
///      keeps the contract deployable today while the real circuit / VK is
///      produced as part of E5 (Groth16 wrap for the Ethereum bridge).
contract Groth16Verifier is IStateProofVerifier {
    /// @dev BN254 base field modulus.
    uint256 internal constant Q =
        21888242871839275222246405745257275088696311157297823662689037894645226208583;
    /// @dev BN254 scalar field modulus (public inputs must be < R).
    uint256 internal constant R =
        21888242871839275222246405745257275088548364400416034343698204186575808495617;

    uint256 internal constant PUBLIC_INPUT_COUNT = 9;

    struct G1Point {
        uint256 x;
        uint256 y;
    }

    /// @dev Encoded as [x.c1, x.c0, y.c1, y.c0] to match the precompile order.
    struct G2Point {
        uint256[2] x;
        uint256[2] y;
    }

    struct VerifyingKey {
        G1Point alpha1;
        G2Point beta2;
        G2Point gamma2;
        G2Point delta2;
        // IC has PUBLIC_INPUT_COUNT + 1 entries.
        G1Point[10] ic;
    }

    address public owner;
    bool public vkLocked;

    VerifyingKey internal vk;

    event VerifyingKeySet(address indexed by);
    event VerifyingKeyLocked();
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    error NotOwner();
    error AlreadyLocked();
    error NotConfigured();
    error BadInputLength();
    error InputOutOfField();
    error PairingFailed();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor() {
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    function transferOwnership(address newOwner) external onlyOwner {
        emit OwnershipTransferred(owner, newOwner);
        owner = newOwner;
    }

    /// @inheritdoc IStateProofVerifier
    function publicInputCount() external pure returns (uint256) {
        return PUBLIC_INPUT_COUNT;
    }

    /// @notice Install the Groth16 verifying key. Callable once, before lock.
    /// @dev `ic` must contain exactly PUBLIC_INPUT_COUNT + 1 (= 10) points.
    function setVerifyingKey(
        G1Point calldata alpha1,
        G2Point calldata beta2,
        G2Point calldata gamma2,
        G2Point calldata delta2,
        G1Point[10] calldata ic
    ) external onlyOwner {
        if (vkLocked) revert AlreadyLocked();
        vk.alpha1 = alpha1;
        vk.beta2 = beta2;
        vk.gamma2 = gamma2;
        vk.delta2 = delta2;
        for (uint256 i = 0; i < 10; i++) {
            vk.ic[i] = ic[i];
        }
        emit VerifyingKeySet(msg.sender);
    }

    /// @notice Permanently lock the verifying key. No further changes allowed.
    function lockVerifyingKey() external onlyOwner {
        if (vkLocked) revert AlreadyLocked();
        if (vk.ic[0].x == 0 && vk.ic[0].y == 0) revert NotConfigured();
        vkLocked = true;
        emit VerifyingKeyLocked();
    }

    /// @notice True once a verifying key has been installed.
    function isConfigured() public view returns (bool) {
        return !(vk.ic[0].x == 0 && vk.ic[0].y == 0);
    }

    /// @inheritdoc IStateProofVerifier
    function verifyProof(uint256[8] calldata proof, uint256[] calldata input)
        external
        view
        returns (bool)
    {
        if (!isConfigured()) revert NotConfigured();
        if (input.length != PUBLIC_INPUT_COUNT) revert BadInputLength();

        // vk_x = IC[0] + sum_i input[i] * IC[i+1]
        G1Point memory vkX = vk.ic[0];
        for (uint256 i = 0; i < PUBLIC_INPUT_COUNT; i++) {
            if (input[i] >= R) revert InputOutOfField();
            vkX = _add(vkX, _scalarMul(vk.ic[i + 1], input[i]));
        }

        G1Point memory a = G1Point(proof[0], proof[1]);
        G2Point memory b = G2Point([proof[2], proof[3]], [proof[4], proof[5]]);
        G1Point memory c = G1Point(proof[6], proof[7]);

        // Check: e(-A, B) * e(alpha, beta) * e(vk_x, gamma) * e(C, delta) == 1
        return _pairingProduct4(_negate(a), b, vk.alpha1, vk.beta2, vkX, vk.gamma2, c, vk.delta2);
    }

    function _negate(G1Point memory p) internal pure returns (G1Point memory) {
        if (p.x == 0 && p.y == 0) {
            return G1Point(0, 0);
        }
        return G1Point(p.x, Q - (p.y % Q));
    }

    function _add(G1Point memory p1, G1Point memory p2)
        internal
        view
        returns (G1Point memory r)
    {
        uint256[4] memory inp;
        inp[0] = p1.x;
        inp[1] = p1.y;
        inp[2] = p2.x;
        inp[3] = p2.y;
        bool ok;
        assembly {
            ok := staticcall(gas(), 0x06, inp, 0x80, r, 0x40)
        }
        if (!ok) revert PairingFailed();
    }

    function _scalarMul(G1Point memory p, uint256 s)
        internal
        view
        returns (G1Point memory r)
    {
        uint256[3] memory inp;
        inp[0] = p.x;
        inp[1] = p.y;
        inp[2] = s;
        bool ok;
        assembly {
            ok := staticcall(gas(), 0x07, inp, 0x60, r, 0x40)
        }
        if (!ok) revert PairingFailed();
    }

    /// @dev Computes the 4-term pairing product and returns whether it equals 1.
    function _pairingProduct4(
        G1Point memory a1,
        G2Point memory a2,
        G1Point memory b1,
        G2Point memory b2,
        G1Point memory c1,
        G2Point memory c2,
        G1Point memory d1,
        G2Point memory d2
    ) internal view returns (bool) {
        uint256[24] memory inp;
        G1Point[4] memory g1 = [a1, b1, c1, d1];
        G2Point[4] memory g2 = [a2, b2, c2, d2];
        for (uint256 i = 0; i < 4; i++) {
            uint256 o = i * 6;
            inp[o + 0] = g1[i].x;
            inp[o + 1] = g1[i].y;
            inp[o + 2] = g2[i].x[0];
            inp[o + 3] = g2[i].x[1];
            inp[o + 4] = g2[i].y[0];
            inp[o + 5] = g2[i].y[1];
        }
        uint256[1] memory out;
        bool ok;
        assembly {
            ok := staticcall(gas(), 0x08, inp, 0x300, out, 0x20)
        }
        if (!ok) revert PairingFailed();
        return out[0] == 1;
    }
}
