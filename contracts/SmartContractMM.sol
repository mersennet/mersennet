// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title On-Chain Market Maker using CLOB Precompile
/// @notice Automated market making with inventory management
/// @dev Grid strategy with N levels on each side, spread adjustment based on position,
///      automatic rebalancing, and P&L tracking.

interface IPrimeOrders {
    function placeOrder(uint64 marketId, uint8 side, uint64 price, uint64 amount, uint8 tif) external returns (uint64);
    function cancelOrder(uint64 orderId) external returns (bool);
    function getPosition(uint64 marketId, address trader) external view returns (int128 size, uint64 avgEntry);
    function depositCollateral(uint64 marketId, uint256 amount) external;
    function withdrawCollateral(uint64 marketId, uint256 amount) external;
}

contract SmartContractMM {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(address(0x0100));

    address public owner;
    uint64 public marketId;
    uint64 public midPrice;
    uint64 public gridSpacing;
    uint8 public numLevels;
    uint64 public sizePerLevel;
    uint64 public maxInventory;
    uint64 public inventorySkewBps;

    struct GridLevel {
        uint64 bidOrderId;
        uint64 askOrderId;
        uint64 bidPrice;
        uint64 askPrice;
        bool active;
    }
    GridLevel[] public levels;

    int128 public netPosition;
    uint256 public realizedPnl;
    uint64 public lastRebalanceBlock;

    mapping(uint256 => uint64) public orderIdToLevel;

    event GridUpdated(uint256 level, uint64 bidId, uint64 askId);
    event PositionUpdated(int128 newPosition);
    event PnLUpdated(uint256 realized);

    constructor(
        uint64 _marketId,
        uint64 _midPrice,
        uint64 _gridSpacing,
        uint8 _numLevels,
        uint64 _sizePerLevel,
        uint64 _maxInventory
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
            levels.push(GridLevel({
                bidOrderId: 0,
                askOrderId: 0,
                bidPrice: 0,
                askPrice: 0,
                active: false
            }));
        }
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    /// @notice Deposit collateral and initialize grid
    function depositAndInit() external payable onlyOwner {
        PRIME_ORDERS.depositCollateral(marketId, msg.value);
        _placeGrid();
    }

    /// @notice Place grid orders around mid price
    function _placeGrid() internal {
        for (uint8 i = 0; i < numLevels; i++) {
            uint64 levelOffset = uint64(i + 1) * gridSpacing;
            uint64 bidPrice = midPrice > levelOffset ? midPrice - levelOffset : 1;
            uint64 askPrice = midPrice + levelOffset;

            if (bidPrice > 0) {
                uint64 bidId = PRIME_ORDERS.placeOrder(marketId, 0, bidPrice, sizePerLevel, 0);
                uint64 askId = PRIME_ORDERS.placeOrder(marketId, 1, askPrice, sizePerLevel, 0);

                levels[i] = GridLevel({
                    bidOrderId: bidId,
                    askOrderId: askId,
                    bidPrice: bidPrice,
                    askPrice: askPrice,
                    active: true
                });
                orderIdToLevel[bidId] = i;
                orderIdToLevel[askId] = i;
                emit GridUpdated(i, bidId, askId);
            }
        }
    }

    function _safeAdd(uint64 a, uint64 b) internal pure returns (uint64) {
        uint64 c = a + b;
        require(c >= a, "overflow");
        return c;
    }

    function _safeSub(uint64 a, uint64 b) internal pure returns (uint64) {
        require(a >= b, "underflow");
        return a - b;
    }

    /// @notice Update mid price and adjust grid (cancel + replace)
    function updateMidPrice(uint64 newMid) external onlyOwner {
        _cancelAll();
        midPrice = newMid;
        _placeGrid();
    }

    /// @notice Cancel all grid orders
    function _cancelAll() internal {
        for (uint8 i = 0; i < numLevels; i++) {
            if (levels[i].active) {
                if (levels[i].bidOrderId > 0) {
                    PRIME_ORDERS.cancelOrder(levels[i].bidOrderId);
                    levels[i].bidOrderId = 0;
                }
                if (levels[i].askOrderId > 0) {
                    PRIME_ORDERS.cancelOrder(levels[i].askOrderId);
                    levels[i].askOrderId = 0;
                }
                levels[i].active = false;
            }
        }
    }

    /// @notice Rebalance: widen spread when position is large
    function rebalance() external onlyOwner {
        (int128 pos, ) = PRIME_ORDERS.getPosition(marketId, address(this));
        netPosition = pos;

        if (uint128(abs(pos)) >= maxInventory) {
            _cancelAll();
            inventorySkewBps = uint128(abs(pos)) > maxInventory * 2 ? 200 : 100;
            gridSpacing = (gridSpacing * (10000 + inventorySkewBps)) / 10000;
            _placeGrid();
        }
        lastRebalanceBlock = uint64(block.number);
        emit PositionUpdated(pos);
    }

    function abs(int128 x) internal pure returns (uint128) {
        return x >= 0 ? uint128(x) : uint128(-x);
    }

    /// @notice Record realized P&L (call after closing position)
    function recordPnl(uint256 amount) external onlyOwner {
        realizedPnl += amount;
        emit PnLUpdated(realizedPnl);
    }

    /// @notice Emergency withdraw
    function emergencyWithdraw(uint256 amount) external onlyOwner {
        _cancelAll();
        PRIME_ORDERS.withdrawCollateral(marketId, amount);
    }

    /// @notice Get current stats
    function getStats() external view returns (
        int128 position,
        uint256 pnl,
        uint8 activeLevels
    ) {
        (position, ) = PRIME_ORDERS.getPosition(marketId, address(this));
        pnl = realizedPnl;
        uint8 count = 0;
        for (uint8 i = 0; i < numLevels; i++) {
            if (levels[i].active) count++;
        }
        activeLevels = count;
    }
}
