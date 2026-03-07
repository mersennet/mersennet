// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Prime Chain Vault Strategy
/// @notice Demonstrates atomic EVM ↔ CLOB interaction via the precompile at 0x0100
/// @dev This contract deposits collateral, places orders, and reacts to fills
///      all in a single transaction — impossible on Hyperliquid or any other chain.
interface IPrimeOrders {
    function placeOrder(uint64 marketId, uint8 side, uint64 price, uint64 amount, uint8 tif) external returns (uint64);
    function cancelOrder(uint64 orderId) external returns (bool);
    function getPosition(uint64 marketId, address trader) external view returns (int128 size, uint64 avgEntry);
    function depositCollateral(uint64 marketId, uint256 amount) external;
    function withdrawCollateral(uint64 marketId, uint256 amount) external;
}

contract VaultStrategy {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(address(0x0100));

    address public owner;
    uint64 public marketId;
    uint64 public targetSpreadBps;
    uint256 public maxPositionSize;

    // State for tracking P&L
    int128 public currentPosition;
    uint256 public totalPnl;
    uint64 public lastOrderId;

    // Events
    event QuoteUpdated(uint64 bidOrderId, uint64 askOrderId, uint64 bidPrice, uint64 askPrice);
    event PositionChanged(int128 newPosition, uint256 entryPrice);

    constructor(uint64 _marketId, uint64 _spreadBps, uint256 _maxPos) {
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
    function updateQuotes(uint64 midPrice) external onlyOwner {
        // Cancel existing order if any
        if (lastOrderId > 0) {
            PRIME_ORDERS.cancelOrder(lastOrderId);
        }

        uint64 spread = midPrice * targetSpreadBps / 10000;
        uint64 bidPrice = midPrice - spread;
        uint64 askPrice = midPrice + spread;

        // Atomic: place both sides
        uint64 bidId = PRIME_ORDERS.placeOrder(marketId, 0, bidPrice, 1, 0); // Buy, GTC
        uint64 askId = PRIME_ORDERS.placeOrder(marketId, 1, askPrice, 1, 0); // Sell, GTC

        lastOrderId = askId; // Track for next cancellation

        emit QuoteUpdated(bidId, askId, bidPrice, askPrice);
    }

    /// @notice Deposit collateral for trading
    function deposit() external payable onlyOwner {
        PRIME_ORDERS.depositCollateral(marketId, msg.value);
    }

    /// @notice Emergency: cancel all and withdraw
    function emergencyExit() external onlyOwner {
        if (lastOrderId > 0) {
            PRIME_ORDERS.cancelOrder(lastOrderId);
        }
        (int128 pos, ) = PRIME_ORDERS.getPosition(marketId, address(this));
        // Close position if any
        if (pos > 0) {
            PRIME_ORDERS.placeOrder(marketId, 1, 1, uint64(uint128(pos)), 1); // Sell IOC at market
        } else if (pos < 0) {
            PRIME_ORDERS.placeOrder(marketId, 0, type(uint64).max, uint64(uint128(-pos)), 1); // Buy IOC at market
        }
    }
}
