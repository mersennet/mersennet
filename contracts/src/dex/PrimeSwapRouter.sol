// SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.8.20;

interface IPrimeSwapFactory {
    function getPair(address tokenA, address tokenB) external view returns (address pair);
    function createPair(address tokenA, address tokenB) external returns (address pair);
}

interface IPrimeSwapPair {
    function getReserves() external view returns (uint112, uint112, uint32);
    function token0() external view returns (address);
    function mint(address to) external returns (uint256 liquidity);
    function burn(address to) external returns (uint256 amount0, uint256 amount1);
    function swap(uint256 amount0Out, uint256 amount1Out, address to, bytes calldata data) external;
}

interface IERC20 {
    function balanceOf(address) external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
    function transferFrom(address, address, uint256) external returns (bool);
}

interface IWPRIM {
    function deposit() external payable;
    function withdraw(uint256) external;
    function transfer(address, uint256) external returns (bool);
}

/// @title PrimeSwap Router
/// @notice Router for adding/removing liquidity and executing swaps
contract PrimeSwapRouter {
    address public immutable factory;
    address public immutable WPRIM;

    modifier ensure(uint256 deadline) {
        require(deadline >= block.timestamp, "PrimeSwapRouter: EXPIRED");
        _;
    }

    constructor(address _factory, address _WPRIM) {
        factory = _factory;
        WPRIM = _WPRIM;
    }

    receive() external payable {
        assert(msg.sender == WPRIM);
    }

    // ========== ADD LIQUIDITY ==========

    function addLiquidity(
        address tokenA, address tokenB,
        uint256 amountADesired, uint256 amountBDesired,
        uint256 amountAMin, uint256 amountBMin,
        address to, uint256 deadline
    ) external ensure(deadline) returns (uint256 amountA, uint256 amountB, uint256 liquidity) {
        (amountA, amountB) = _computeLiquidityAmounts(tokenA, tokenB, amountADesired, amountBDesired, amountAMin, amountBMin);
        address pair = _pairFor(tokenA, tokenB);
        _safeTransferFrom(tokenA, msg.sender, pair, amountA);
        _safeTransferFrom(tokenB, msg.sender, pair, amountB);
        liquidity = IPrimeSwapPair(pair).mint(to);
    }

    function addLiquidityPRIM(
        address token,
        uint256 amountTokenDesired, uint256 amountTokenMin, uint256 amountPRIMMin,
        address to, uint256 deadline
    ) external payable ensure(deadline) returns (uint256 amountToken, uint256 amountPRIM, uint256 liquidity) {
        (amountToken, amountPRIM) = _computeLiquidityAmounts(token, WPRIM, amountTokenDesired, msg.value, amountTokenMin, amountPRIMMin);
        address pair = _pairFor(token, WPRIM);
        _safeTransferFrom(token, msg.sender, pair, amountToken);
        IWPRIM(WPRIM).deposit{value: amountPRIM}();
        assert(IWPRIM(WPRIM).transfer(pair, amountPRIM));
        liquidity = IPrimeSwapPair(pair).mint(to);
        if (msg.value > amountPRIM) {
            payable(msg.sender).transfer(msg.value - amountPRIM);
        }
    }

    // ========== REMOVE LIQUIDITY ==========

    function removeLiquidity(
        address tokenA, address tokenB,
        uint256 liquidity,
        uint256 amountAMin, uint256 amountBMin,
        address to, uint256 deadline
    ) public ensure(deadline) returns (uint256 amountA, uint256 amountB) {
        address pair = _pairFor(tokenA, tokenB);
        IERC20(pair).transferFrom(msg.sender, pair, liquidity);
        (uint256 amount0, uint256 amount1) = IPrimeSwapPair(pair).burn(to);
        (address token0,) = _sortTokens(tokenA, tokenB);
        (amountA, amountB) = tokenA == token0 ? (amount0, amount1) : (amount1, amount0);
        require(amountA >= amountAMin, "PrimeSwapRouter: INSUFFICIENT_A_AMOUNT");
        require(amountB >= amountBMin, "PrimeSwapRouter: INSUFFICIENT_B_AMOUNT");
    }

    function removeLiquidityPRIM(
        address token,
        uint256 liquidity,
        uint256 amountTokenMin, uint256 amountPRIMMin,
        address to, uint256 deadline
    ) external ensure(deadline) returns (uint256 amountToken, uint256 amountPRIM) {
        (amountToken, amountPRIM) = removeLiquidity(token, WPRIM, liquidity, amountTokenMin, amountPRIMMin, address(this), deadline);
        _safeTransfer(token, to, amountToken);
        IWPRIM(WPRIM).withdraw(amountPRIM);
        payable(to).transfer(amountPRIM);
    }

    // ========== SWAP ==========

    function swapExactTokensForTokens(
        uint256 amountIn, uint256 amountOutMin,
        address[] calldata path, address to, uint256 deadline
    ) external ensure(deadline) returns (uint256[] memory amounts) {
        amounts = getAmountsOut(amountIn, path);
        require(amounts[amounts.length - 1] >= amountOutMin, "PrimeSwapRouter: INSUFFICIENT_OUTPUT_AMOUNT");
        _safeTransferFrom(path[0], msg.sender, _pairFor(path[0], path[1]), amounts[0]);
        _swap(amounts, path, to);
    }

    function swapExactPRIMForTokens(
        uint256 amountOutMin, address[] calldata path, address to, uint256 deadline
    ) external payable ensure(deadline) returns (uint256[] memory amounts) {
        require(path[0] == WPRIM, "PrimeSwapRouter: INVALID_PATH");
        amounts = getAmountsOut(msg.value, path);
        require(amounts[amounts.length - 1] >= amountOutMin, "PrimeSwapRouter: INSUFFICIENT_OUTPUT_AMOUNT");
        IWPRIM(WPRIM).deposit{value: amounts[0]}();
        assert(IWPRIM(WPRIM).transfer(_pairFor(path[0], path[1]), amounts[0]));
        _swap(amounts, path, to);
    }

    function swapExactTokensForPRIM(
        uint256 amountIn, uint256 amountOutMin,
        address[] calldata path, address to, uint256 deadline
    ) external ensure(deadline) returns (uint256[] memory amounts) {
        require(path[path.length - 1] == WPRIM, "PrimeSwapRouter: INVALID_PATH");
        amounts = getAmountsOut(amountIn, path);
        require(amounts[amounts.length - 1] >= amountOutMin, "PrimeSwapRouter: INSUFFICIENT_OUTPUT_AMOUNT");
        _safeTransferFrom(path[0], msg.sender, _pairFor(path[0], path[1]), amounts[0]);
        _swap(amounts, path, address(this));
        IWPRIM(WPRIM).withdraw(amounts[amounts.length - 1]);
        payable(to).transfer(amounts[amounts.length - 1]);
    }

    // ========== LIBRARY FUNCTIONS ==========

    function quote(uint256 amountA, uint256 reserveA, uint256 reserveB) public pure returns (uint256) {
        require(amountA > 0, "PrimeSwapRouter: INSUFFICIENT_AMOUNT");
        require(reserveA > 0 && reserveB > 0, "PrimeSwapRouter: INSUFFICIENT_LIQUIDITY");
        return (amountA * reserveB) / reserveA;
    }

    function getAmountOut(uint256 amountIn, uint256 reserveIn, uint256 reserveOut) public pure returns (uint256) {
        require(amountIn > 0, "PrimeSwapRouter: INSUFFICIENT_INPUT_AMOUNT");
        require(reserveIn > 0 && reserveOut > 0, "PrimeSwapRouter: INSUFFICIENT_LIQUIDITY");
        uint256 amountInWithFee = amountIn * 997;
        return (amountInWithFee * reserveOut) / (reserveIn * 1000 + amountInWithFee);
    }

    function getAmountIn(uint256 amountOut, uint256 reserveIn, uint256 reserveOut) public pure returns (uint256) {
        require(amountOut > 0, "PrimeSwapRouter: INSUFFICIENT_OUTPUT_AMOUNT");
        require(reserveIn > 0 && reserveOut > 0, "PrimeSwapRouter: INSUFFICIENT_LIQUIDITY");
        return (reserveIn * amountOut * 1000) / ((reserveOut - amountOut) * 997) + 1;
    }

    function getAmountsOut(uint256 amountIn, address[] memory path) public view returns (uint256[] memory amounts) {
        require(path.length >= 2, "PrimeSwapRouter: INVALID_PATH");
        amounts = new uint256[](path.length);
        amounts[0] = amountIn;
        for (uint256 i; i < path.length - 1; i++) {
            (uint256 reserveIn, uint256 reserveOut) = _getReserves(path[i], path[i + 1]);
            amounts[i + 1] = getAmountOut(amounts[i], reserveIn, reserveOut);
        }
    }

    function getAmountsIn(uint256 amountOut, address[] memory path) public view returns (uint256[] memory amounts) {
        require(path.length >= 2, "PrimeSwapRouter: INVALID_PATH");
        amounts = new uint256[](path.length);
        amounts[amounts.length - 1] = amountOut;
        for (uint256 i = path.length - 1; i > 0; i--) {
            (uint256 reserveIn, uint256 reserveOut) = _getReserves(path[i - 1], path[i]);
            amounts[i - 1] = getAmountIn(amounts[i], reserveIn, reserveOut);
        }
    }

    // ========== INTERNAL ==========

    function _swap(uint256[] memory amounts, address[] memory path, address _to) internal {
        for (uint256 i; i < path.length - 1; i++) {
            (address input, address output) = (path[i], path[i + 1]);
            (address token0,) = _sortTokens(input, output);
            uint256 amountOut = amounts[i + 1];
            (uint256 amount0Out, uint256 amount1Out) = input == token0 ? (uint256(0), amountOut) : (amountOut, uint256(0));
            address to = i < path.length - 2 ? _pairFor(output, path[i + 2]) : _to;
            IPrimeSwapPair(_pairFor(input, output)).swap(amount0Out, amount1Out, to, new bytes(0));
        }
    }

    function _computeLiquidityAmounts(
        address tokenA, address tokenB,
        uint256 amountADesired, uint256 amountBDesired,
        uint256 amountAMin, uint256 amountBMin
    ) internal returns (uint256 amountA, uint256 amountB) {
        if (IPrimeSwapFactory(factory).getPair(tokenA, tokenB) == address(0)) {
            IPrimeSwapFactory(factory).createPair(tokenA, tokenB);
        }
        (uint256 reserveA, uint256 reserveB) = _getReserves(tokenA, tokenB);
        if (reserveA == 0 && reserveB == 0) {
            (amountA, amountB) = (amountADesired, amountBDesired);
        } else {
            uint256 amountBOptimal = quote(amountADesired, reserveA, reserveB);
            if (amountBOptimal <= amountBDesired) {
                require(amountBOptimal >= amountBMin, "PrimeSwapRouter: INSUFFICIENT_B_AMOUNT");
                (amountA, amountB) = (amountADesired, amountBOptimal);
            } else {
                uint256 amountAOptimal = quote(amountBDesired, reserveB, reserveA);
                assert(amountAOptimal <= amountADesired);
                require(amountAOptimal >= amountAMin, "PrimeSwapRouter: INSUFFICIENT_A_AMOUNT");
                (amountA, amountB) = (amountAOptimal, amountBDesired);
            }
        }
    }

    function _getReserves(address tokenA, address tokenB) internal view returns (uint256 reserveA, uint256 reserveB) {
        (address token0,) = _sortTokens(tokenA, tokenB);
        address pair = IPrimeSwapFactory(factory).getPair(tokenA, tokenB);
        if (pair == address(0)) return (0, 0);
        (uint112 r0, uint112 r1,) = IPrimeSwapPair(pair).getReserves();
        (reserveA, reserveB) = tokenA == token0 ? (uint256(r0), uint256(r1)) : (uint256(r1), uint256(r0));
    }

    function _pairFor(address tokenA, address tokenB) internal view returns (address) {
        return IPrimeSwapFactory(factory).getPair(tokenA, tokenB);
    }

    function _sortTokens(address tokenA, address tokenB) internal pure returns (address token0, address token1) {
        (token0, token1) = tokenA < tokenB ? (tokenA, tokenB) : (tokenB, tokenA);
    }

    function _safeTransfer(address token, address to, uint256 value) private {
        (bool ok, bytes memory data) = token.call(abi.encodeWithSelector(0xa9059cbb, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "PrimeSwapRouter: TRANSFER_FAILED");
    }

    function _safeTransferFrom(address token, address from, address to, uint256 value) private {
        (bool ok, bytes memory data) = token.call(abi.encodeWithSelector(0x23b872dd, from, to, value));
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "PrimeSwapRouter: TRANSFER_FROM_FAILED");
    }
}
