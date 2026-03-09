// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/foundation/MockERC20.sol";

contract MockERC20Test is Test {
    MockERC20 public token;
    address public alice = makeAddr("alice");
    address public bob = makeAddr("bob");

    function setUp() public {
        token = new MockERC20("Test Token", "TEST", 18);
    }

    function test_metadata() public view {
        assertEq(token.name(), "Test Token");
        assertEq(token.symbol(), "TEST");
        assertEq(token.decimals(), 18);
    }

    function test_mint() public {
        token.mint(alice, 1000e18);
        assertEq(token.balanceOf(alice), 1000e18);
        assertEq(token.totalSupply(), 1000e18);
    }

    function test_mint_max() public {
        token.mint(alice, 10_000e18);
        assertEq(token.balanceOf(alice), 10_000e18);
    }

    function test_mint_exceeds_max() public {
        vm.expectRevert("MockERC20: max 10,000 tokens per mint");
        token.mint(alice, 10_001e18);
    }

    function test_faucet() public {
        vm.prank(alice);
        token.faucet();
        assertEq(token.balanceOf(alice), 10_000e18);
    }

    function test_transfer() public {
        token.mint(alice, 1000e18);
        vm.prank(alice);
        token.transfer(bob, 400e18);
        assertEq(token.balanceOf(alice), 600e18);
        assertEq(token.balanceOf(bob), 400e18);
    }

    function test_transfer_insufficient() public {
        token.mint(alice, 100e18);
        vm.prank(alice);
        vm.expectRevert("MockERC20: insufficient balance");
        token.transfer(bob, 200e18);
    }

    function test_approve_and_transferFrom() public {
        token.mint(alice, 1000e18);
        vm.prank(alice);
        token.approve(bob, 500e18);
        assertEq(token.allowance(alice, bob), 500e18);

        vm.prank(bob);
        token.transferFrom(alice, bob, 300e18);
        assertEq(token.balanceOf(bob), 300e18);
        assertEq(token.allowance(alice, bob), 200e18);
    }

    function test_transferFrom_max_allowance() public {
        token.mint(alice, 1000e18);
        vm.prank(alice);
        token.approve(bob, type(uint256).max);

        vm.prank(bob);
        token.transferFrom(alice, bob, 500e18);
        assertEq(token.allowance(alice, bob), type(uint256).max);
    }

    function test_6_decimals() public {
        MockERC20 usdc = new MockERC20("USD Coin", "USDC", 6);
        assertEq(usdc.decimals(), 6);
        usdc.mint(alice, 10_000e6);
        assertEq(usdc.balanceOf(alice), 10_000e6);
    }

    function test_faucet_6_decimals() public {
        MockERC20 usdc = new MockERC20("USD Coin", "USDC", 6);
        vm.prank(alice);
        usdc.faucet();
        assertEq(usdc.balanceOf(alice), 10_000e6);
    }
}
