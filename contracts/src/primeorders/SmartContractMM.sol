// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../interfaces/IPrimeOrders.sol";

/// @title On-Chain Market Maker using CLOB Precompile
/// @notice Automated market making with inventory management
/// @dev Grid strategy with N levels on each side, spread adjustment based on position,
///      automatic rebalancing, and P&L tracking.
contract SmartContractMM {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(PRIME_ORDERS_ADDRESS);

    address public owner;
    uint64 public marketId;
    uint256 public midPrice;
    uint256 public gridSpacing;
    uint8 public numLevels;
    uint256 public sizePerLevel;
    uint256 public maxInventory;
    uint256 public inventorySkewBps;

    struct GridLevel {
        uint256 bidOrderId;
        uint256 askOrderId;
        uint256 bidPrice;
        uint256 askPrice;
        bool active;
    }
    GridLevel[] public levels;

    int128 public netPosition;
    uint256 public realizedPnl;
    uint256 public lastRebalanceBlock;

    event GridUpdated(uint256 level, uint256 bidId, uint256 askId);
    event PositionUpdated(int128 newPosition, uint256 collateral);
    event PnLUpdated(uint256 realized);

    constructor(
        uint64 _marketId,
        uint256 _midPrice,
        uint256 _gridSpacing,
        uint8 _numLevels,
        uint256 _sizePerLevel,
        uint256 _maxInventory
    ) {
        owner = msg.sender;
        marketId = _marketId;
        midPrice = _midPrice;
        gridSpacing = _gridSpacing;
        numLevels = _numLevels;
        sizePerLevel = _sizePerLevel;
        maxInventory = _maxInventory;
        inventorySkewBps = 50;

        for (uint8 i = 0; i < _numLevels; i++) {
            levels.push(GridLevel(0, 0, 0, 0, false));
        }
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    /// @notice Deposit collateral and initialize grid
    function depositAndInit() external payable onlyOwner {
        PRIME_ORDERS.depositCollateral(msg.value);
        _placeGrid();
    }

    function _placeGrid() internal {
        for (uint8 i = 0; i < numLevels; i++) {
            uint256 levelOffset = uint256(i + 1) * gridSpacing;
            uint256 bidPrice = midPrice > levelOffset ? midPrice - levelOffset : 1;
            uint256 askPrice = midPrice + levelOffset;

            (uint256 bidId,,) = PRIME_ORDERS.placeOrder(marketId, true, bidPrice, sizePerLevel, 0);
            (uint256 askId,,) = PRIME_ORDERS.placeOrder(marketId, false, askPrice, sizePerLevel, 0);

            levels[i] = GridLevel({
                bidOrderId: bidId,
                askOrderId: askId,
                bidPrice: bidPrice,
                askPrice: askPrice,
                active: true
            });
            emit GridUpdated(i, bidId, askId);
        }
    }

    /// @notice Update mid price and adjust grid
    function updateMidPrice(uint256 newMid) external onlyOwner {
        _cancelAll();
        midPrice = newMid;
        _placeGrid();
    }

    function _cancelAll() internal {
        for (uint8 i = 0; i < numLevels; i++) {
            if (levels[i].active) {
                if (levels[i].bidOrderId > 0) PRIME_ORDERS.cancelOrder(levels[i].bidOrderId);
                if (levels[i].askOrderId > 0) PRIME_ORDERS.cancelOrder(levels[i].askOrderId);
                levels[i].active = false;
            }
        }
    }

    /// @notice Rebalance: widen spread when inventory is large
    function rebalance() external onlyOwner {
        (int128 pos,) = PRIME_ORDERS.getPosition(marketId);
        netPosition = pos;

        uint256 absPos = pos >= 0 ? uint256(uint128(pos)) : uint256(uint128(-pos));
        if (absPos >= maxInventory) {
            _cancelAll();
            inventorySkewBps = absPos > maxInventory * 2 ? 200 : 100;
            gridSpacing = (gridSpacing * (10000 + inventorySkewBps)) / 10000;
            _placeGrid();
        }
        lastRebalanceBlock = block.number;

        uint256 col = PRIME_ORDERS.getCollateral();
        emit PositionUpdated(pos, col);
    }

    /// @notice Emergency: cancel everything and withdraw
    function emergencyWithdraw() external onlyOwner {
        _cancelAll();
        uint256 col = PRIME_ORDERS.getCollateral();
        if (col > 0) {
            PRIME_ORDERS.withdrawCollateral(col);
            (bool success,) = payable(owner).call{value: col}("");
            require(success, "Transfer failed");
        }
    }

    /// @notice Get current status
    function getStats() external view returns (
        int128 position,
        uint256 entryPrice,
        uint256 collateral,
        uint256 pnl,
        uint8 activeLevels
    ) {
        (position, entryPrice) = PRIME_ORDERS.getPosition(marketId);
        collateral = PRIME_ORDERS.getCollateral();
        pnl = realizedPnl;
        uint8 count = 0;
        for (uint8 i = 0; i < numLevels; i++) {
            if (levels[i].active) count++;
        }
        activeLevels = count;
    }
}
