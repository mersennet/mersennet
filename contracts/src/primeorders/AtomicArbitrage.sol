// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../interfaces/IPrimeOrders.sol";

/// @title Atomic Arbitrage across CLOB and EVM pools
/// @notice Buy on CLOB, sell on AMM (or vice versa) atomically
/// @dev Entire operation is a single transaction — reverts if any step fails.
///      This pattern is only possible because PrimeOrders executes synchronously.
contract AtomicArbitrage {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(PRIME_ORDERS_ADDRESS);

    address public immutable owner;
    address public immutable baseToken;
    address public immutable quoteToken;

    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }

    event ArbitrageExecuted(
        uint64 marketId,
        uint256 clobFilled,
        uint256 ammOut,
        int256 profit
    );

    constructor(address _baseToken, address _quoteToken) {
        owner = msg.sender;
        baseToken = _baseToken;
        quoteToken = _quoteToken;
    }

    /// @notice Execute arbitrage: buy on CLOB at limit price, sell on AMM
    /// @param marketId    CLOB market to trade
    /// @param buyPrice    Max price to pay on CLOB (18 decimals)
    /// @param size        Amount to buy (18 decimals)
    /// @param ammPool     AMM router/pool to sell into
    /// @param minAmountOut Minimum output from AMM swap
    function clobToAmm(
        uint64 marketId,
        uint256 buyPrice,
        uint256 size,
        address ammPool,
        uint256 minAmountOut
    ) external {
        require(ammPool != address(0), "Invalid AMM pool");
        require(ammPool.code.length > 0, "AMM pool has no code");

        (, uint256 filled,) = PRIME_ORDERS.placeOrder(marketId, true, buyPrice, size, 1); // IOC
        require(filled > 0, "CLOB order not filled");

        address[] memory path = new address[](2);
        path[0] = baseToken;
        path[1] = quoteToken;

        (bool success, bytes memory result) = ammPool.call(
            abi.encodeWithSignature(
                "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)",
                filled, minAmountOut, path, address(this), block.timestamp
            )
        );
        require(success, "AMM swap failed");

        emit ArbitrageExecuted(marketId, filled, abi.decode(result, (uint256)), 0);
    }

    /// @notice Reverse: buy on AMM, sell on CLOB
    function ammToClob(
        uint64 marketId,
        uint256 sellPrice,
        uint256 size,
        address ammPool,
        uint256 maxAmountIn
    ) external {
        require(ammPool != address(0), "Invalid AMM pool");
        require(ammPool.code.length > 0, "AMM pool has no code");

        address[] memory path = new address[](2);
        path[0] = quoteToken;
        path[1] = baseToken;

        (bool success,) = ammPool.call(
            abi.encodeWithSignature(
                "swapTokensForExactTokens(uint256,uint256,address[],address,uint256)",
                size, maxAmountIn, path, address(this), block.timestamp
            )
        );
        require(success, "AMM buy failed");

        (, uint256 filled,) = PRIME_ORDERS.placeOrder(marketId, false, sellPrice, size, 1); // Sell IOC
        require(filled > 0, "CLOB sell not filled");

        emit ArbitrageExecuted(marketId, filled, size, 0);
    }

    /// @notice Deposit collateral for CLOB trading
    function depositCollateral() external payable {
        PRIME_ORDERS.depositCollateral(msg.value);
    }

    /// @notice Withdraw collateral
    function withdrawCollateral(uint256 amount) external onlyOwner {
        PRIME_ORDERS.withdrawCollateral(amount);
        (bool success,) = payable(owner).call{value: amount}("");
        require(success, "Transfer failed");
    }
}
