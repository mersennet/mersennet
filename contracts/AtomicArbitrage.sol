// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Atomic Arbitrage across CLOB and EVM pools
/// @notice Buy on CLOB, sell on AMM (or vice versa) atomically

interface IPrimeOrders {
    function placeOrder(uint64 marketId, uint8 side, uint64 price, uint64 amount, uint8 tif) external returns (uint64);
    function getPosition(uint64 marketId, address trader) external view returns (int128 size, uint64 avgEntry);
}

contract AtomicArbitrage {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(address(0x0100));

    /// @notice Execute arbitrage: buy on CLOB at limit price, sell on AMM
    /// @dev Entire operation is atomic — if any step fails, all revert
    function arbitrage(
        uint64 marketId,
        uint64 buyPrice,
        uint64 amount,
        address ammPool,
        uint256 minAmountOut
    ) external {
        // Step 1: Buy on CLOB (atomic, fills in same tx!)
        uint64 orderId = PRIME_ORDERS.placeOrder(marketId, 0, buyPrice, amount, 1); // IOC

        // Step 2: Check we got filled
        (int128 pos, ) = PRIME_ORDERS.getPosition(marketId, address(this));
        require(pos > 0, "CLOB order not filled");

        // Step 3: Sell on AMM pool
        // (simplified - in reality would call the AMM's swap function)
        (bool success, ) = ammPool.call(
            abi.encodeWithSignature("swap(uint256,uint256)", uint256(uint128(pos)), minAmountOut)
        );
        require(success, "AMM swap failed");
    }
}
