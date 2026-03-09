// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../interfaces/IPrimeOrders.sol";

/// @title Prime Chain Vault Strategy
/// @notice Demonstrates atomic EVM + CLOB interaction via the precompile at 0x0100
/// @dev Deposits collateral, places orders, and reacts to fills
///      all in a single transaction -- impossible on Hyperliquid or any other chain.
contract VaultStrategy {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(PRIME_ORDERS_ADDRESS);

    address public owner;
    uint64 public marketId;
    uint256 public targetSpreadBps;
    uint256 public maxPositionSize;

    int128 public currentPosition;
    uint256 public totalPnl;
    uint256 public lastBidOrderId;
    uint256 public lastAskOrderId;

    event QuoteUpdated(uint256 bidOrderId, uint256 askOrderId, uint256 bidPrice, uint256 askPrice);
    event PositionChanged(int128 newPosition, uint256 entryPrice);

    constructor(uint64 _marketId, uint256 _spreadBps, uint256 _maxPos) {
        owner = msg.sender;
        marketId = _marketId;
        targetSpreadBps = _spreadBps;
        maxPositionSize = _maxPos;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    /// @notice Update quotes around a reference price
    /// @dev Atomic: cancel old orders + place new ones in single tx
    function updateQuotes(uint256 midPrice) external onlyOwner {
        if (lastBidOrderId > 0) PRIME_ORDERS.cancelOrder(lastBidOrderId);
        if (lastAskOrderId > 0) PRIME_ORDERS.cancelOrder(lastAskOrderId);

        uint256 spread = midPrice * targetSpreadBps / 10000;
        uint256 bidPrice = midPrice - spread;
        uint256 askPrice = midPrice + spread;

        (uint256 bidId,,) = PRIME_ORDERS.placeOrder(marketId, true, bidPrice, 1 ether, 0);
        (uint256 askId,,) = PRIME_ORDERS.placeOrder(marketId, false, askPrice, 1 ether, 0);

        lastBidOrderId = bidId;
        lastAskOrderId = askId;

        emit QuoteUpdated(bidId, askId, bidPrice, askPrice);
    }

    /// @notice Deposit collateral for trading
    function deposit() external payable onlyOwner {
        PRIME_ORDERS.depositCollateral(msg.value);
    }

    /// @notice Withdraw collateral
    function withdraw(uint256 amount) external onlyOwner {
        PRIME_ORDERS.withdrawCollateral(amount);
        (bool success,) = payable(owner).call{value: amount}("");
        require(success, "Transfer failed");
    }

    /// @notice Check current position and collateral
    function status() external view returns (int128 pos, uint256 entry, uint256 collateral) {
        (pos, entry) = PRIME_ORDERS.getPosition(marketId);
        collateral = PRIME_ORDERS.getCollateral();
    }

    /// @notice Emergency: cancel all and close position
    function emergencyExit() external onlyOwner {
        if (lastBidOrderId > 0) PRIME_ORDERS.cancelOrder(lastBidOrderId);
        if (lastAskOrderId > 0) PRIME_ORDERS.cancelOrder(lastAskOrderId);

        (int128 pos,) = PRIME_ORDERS.getPosition(marketId);
        if (pos > 0) {
            PRIME_ORDERS.placeOrder(marketId, false, 1, uint256(uint128(pos)), 1);
        } else if (pos < 0) {
            PRIME_ORDERS.placeOrder(marketId, true, type(uint256).max, uint256(uint128(-pos)), 1);
        }

        uint256 col = PRIME_ORDERS.getCollateral();
        if (col > 0) {
            PRIME_ORDERS.withdrawCollateral(col);
            (bool success,) = payable(owner).call{value: col}("");
            require(success, "Transfer failed");
        }
    }
}
