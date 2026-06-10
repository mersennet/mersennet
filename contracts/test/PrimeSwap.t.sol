// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/foundation/WMRSN.sol";
import "../src/foundation/MockERC20.sol";
import "../src/dex/PrimeSwapFactory.sol";
import "../src/dex/PrimeSwapRouter.sol";
import "../src/dex/PrimeSwapPair.sol";

contract PrimeSwapTest is Test {
    WMRSN public wprim;
    MockERC20 public usdc;
    MockERC20 public dai;
    PrimeSwapFactory public factory;
    PrimeSwapRouter public router;
    address public alice = makeAddr("alice");

    function setUp() public {
        wprim = new WMRSN();
        usdc = new MockERC20("USD Coin", "USDC", 6);
        dai = new MockERC20("Dai", "DAI", 18);
        factory = new PrimeSwapFactory(address(this));
        router = new PrimeSwapRouter(address(factory), address(wprim));

        vm.deal(alice, 1000 ether);
        // MockERC20 limits to 10k tokens per mint, so mint in batches
        for (uint i = 0; i < 10; i++) {
            usdc.mint(alice, 10_000e6);
            dai.mint(alice, 10_000e18);
        }
    }

    function test_factory_createPair() public {
        address pair = factory.createPair(address(usdc), address(dai));
        assertTrue(pair != address(0));
        assertEq(factory.allPairsLength(), 1);
        assertEq(factory.getPair(address(usdc), address(dai)), pair);
        assertEq(factory.getPair(address(dai), address(usdc)), pair);
    }

    function test_factory_duplicate_pair() public {
        factory.createPair(address(usdc), address(dai));
        vm.expectRevert();
        factory.createPair(address(usdc), address(dai));
    }

    function test_addLiquidityPRIM() public {
        vm.startPrank(alice);
        usdc.approve(address(router), type(uint256).max);

        router.addLiquidityPRIM{value: 10 ether}(
            address(usdc),
            10_000e6,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        address pair = factory.getPair(address(usdc), address(wprim));
        assertTrue(pair != address(0));
        uint256 lpBalance = PrimeSwapPair(pair).balanceOf(alice);
        assertTrue(lpBalance > 0, "LP tokens should be minted");
        vm.stopPrank();
    }

    function test_swapExactPRIMForTokens() public {
        vm.startPrank(alice);
        usdc.approve(address(router), type(uint256).max);

        router.addLiquidityPRIM{value: 100 ether}(
            address(usdc),
            100_000e6,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        uint256 usdcBefore = usdc.balanceOf(alice);
        address[] memory path = new address[](2);
        path[0] = address(wprim);
        path[1] = address(usdc);

        router.swapExactPRIMForTokens{value: 1 ether}(
            0,
            path,
            alice,
            block.timestamp + 1000
        );

        uint256 usdcAfter = usdc.balanceOf(alice);
        assertTrue(usdcAfter > usdcBefore, "Should receive USDC");
        vm.stopPrank();
    }

    function test_swapExactTokensForPRIM() public {
        vm.startPrank(alice);
        usdc.approve(address(router), type(uint256).max);

        router.addLiquidityPRIM{value: 100 ether}(
            address(usdc),
            50_000e6,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        // Re-approve in case allowance was consumed
        usdc.approve(address(router), type(uint256).max);

        uint256 primBefore = alice.balance;
        address[] memory path = new address[](2);
        path[0] = address(usdc);
        path[1] = address(wprim);

        router.swapExactTokensForPRIM(
            1000e6,
            0,
            path,
            alice,
            block.timestamp + 1000
        );

        uint256 primAfter = alice.balance;
        assertTrue(primAfter > primBefore, "Should receive MRSN");
        vm.stopPrank();
    }

    function test_removeLiquidityPRIM() public {
        vm.startPrank(alice);
        usdc.approve(address(router), type(uint256).max);

        router.addLiquidityPRIM{value: 10 ether}(
            address(usdc),
            10_000e6,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        address pair = factory.getPair(address(usdc), address(wprim));
        uint256 lp = PrimeSwapPair(pair).balanceOf(alice);
        assertTrue(lp > 0);

        PrimeSwapPair(pair).approve(address(router), lp);

        uint256 usdcBefore = usdc.balanceOf(alice);
        uint256 primBefore = alice.balance;

        router.removeLiquidityPRIM(
            address(usdc),
            lp,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        assertTrue(usdc.balanceOf(alice) > usdcBefore, "Should get USDC back");
        assertTrue(alice.balance > primBefore, "Should get MRSN back");
        vm.stopPrank();
    }

    function test_addLiquidity_token_pair() public {
        vm.startPrank(alice);
        usdc.approve(address(router), type(uint256).max);
        dai.approve(address(router), type(uint256).max);

        router.addLiquidity(
            address(usdc),
            address(dai),
            10_000e6,
            10_000e18,
            0,
            0,
            alice,
            block.timestamp + 1000
        );

        address pair = factory.getPair(address(usdc), address(dai));
        uint256 lpBalance = PrimeSwapPair(pair).balanceOf(alice);
        assertTrue(lpBalance > 0);
        vm.stopPrank();
    }
}
