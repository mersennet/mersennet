// SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.8.20;

import "./PrimeSwapPair.sol";

/// @title PrimeSwap Factory
/// @notice Creates and manages liquidity pair contracts (Uniswap V2 fork)
contract PrimeSwapFactory {
    address public feeTo;
    address public feeToSetter;

    mapping(address => mapping(address => address)) public getPair;
    address[] public allPairs;

    event PairCreated(address indexed token0, address indexed token1, address pair, uint256 pairCount);

    constructor(address _feeToSetter) {
        feeToSetter = _feeToSetter;
    }

    function allPairsLength() external view returns (uint256) {
        return allPairs.length;
    }

    function createPair(address tokenA, address tokenB) external returns (address pair) {
        require(tokenA != tokenB, "PrimeSwap: IDENTICAL_ADDRESSES");
        (address token0, address token1) = tokenA < tokenB ? (tokenA, tokenB) : (tokenB, tokenA);
        require(token0 != address(0), "PrimeSwap: ZERO_ADDRESS");
        require(getPair[token0][token1] == address(0), "PrimeSwap: PAIR_EXISTS");

        bytes memory bytecode = type(PrimeSwapPair).creationCode;
        bytes32 salt = keccak256(abi.encodePacked(token0, token1));
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        require(pair != address(0), "PrimeSwap: CREATE2_FAILED");

        PrimeSwapPair(pair).initialize(token0, token1);
        getPair[token0][token1] = pair;
        getPair[token1][token0] = pair;
        allPairs.push(pair);
        emit PairCreated(token0, token1, pair, allPairs.length);
    }

    function setFeeTo(address _feeTo) external {
        require(msg.sender == feeToSetter, "PrimeSwap: FORBIDDEN");
        feeTo = _feeTo;
    }

    function setFeeToSetter(address _feeToSetter) external {
        require(msg.sender == feeToSetter, "PrimeSwap: FORBIDDEN");
        feeToSetter = _feeToSetter;
    }

    function pairCodeHash() external pure returns (bytes32) {
        return keccak256(type(PrimeSwapPair).creationCode);
    }
}
