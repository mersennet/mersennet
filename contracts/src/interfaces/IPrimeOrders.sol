// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IPrimeOrders — Canonical interface for the PrimeOrders CLOB precompile
/// @notice Deployed at 0x0000000000000000000000000000000000000100
/// @dev All functions use standard Solidity ABI encoding (4-byte selector + 32-byte words).
///      The precompile is stateful: collateral and positions are tracked per msg.sender.
interface IPrimeOrders {
    /// @notice Place a limit order on the on-chain order book
    /// @param marketId Numeric market identifier (e.g. 1 = PRIM/USDC)
    /// @param isBuy    true = buy, false = sell
    /// @param price    Price in quote-asset units (18 decimals)
    /// @param size     Order size in base-asset units (18 decimals)
    /// @param tif      Time-in-force: 0 = GTC, 1 = IOC, 2 = FOK
    /// @return orderId  Unique order identifier
    /// @return filled   Amount filled immediately
    /// @return remaining Amount left on the book (0 for IOC/FOK)
    function placeOrder(
        uint64 marketId,
        bool isBuy,
        uint256 price,
        uint256 size,
        uint8 tif
    ) external returns (uint256 orderId, uint256 filled, uint256 remaining);

    /// @notice Cancel an open order
    /// @param orderId The order to cancel
    /// @return success true if the order was cancelled
    function cancelOrder(uint256 orderId) external returns (bool success);

    /// @notice Deposit native PRIM as trading collateral
    /// @param amount Amount in wei to deposit (must match msg.value)
    /// @return success true on success
    function depositCollateral(uint256 amount) external returns (bool success);

    /// @notice Withdraw collateral back to msg.sender
    /// @param amount Amount in wei to withdraw
    /// @return success true on success
    function withdrawCollateral(uint256 amount) external returns (bool success);

    /// @notice Get the caller's position for a market
    /// @param marketId The market to query
    /// @return size       Signed position size (positive = long, negative = short)
    /// @return entryPrice Volume-weighted average entry price
    function getPosition(uint64 marketId) external view returns (int128 size, uint256 entryPrice);

    /// @notice Get the caller's total collateral balance
    /// @return collateral Amount in wei
    function getCollateral() external view returns (uint256 collateral);

    /// @notice Check if an account can be liquidated
    /// @param account Address to check
    /// @return true if the account's margin ratio is below maintenance
    function isLiquidatable(address account) external view returns (bool);

    /// @notice Get the current best bid and ask for a market
    /// @param marketId The market to query
    /// @return bestBid Highest bid price (0 if no bids)
    /// @return bestAsk Lowest ask price (0 if no asks)
    function getBestBidAsk(uint64 marketId) external view returns (uint256 bestBid, uint256 bestAsk);
}

/// @dev Precompile address constant for convenience
address constant PRIME_ORDERS_ADDRESS = 0x0000000000000000000000000000000000000100;
