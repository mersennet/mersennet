// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/foundation/WPRIM.sol";

contract WPRIMTest is Test {
    WPRIM public wprim;
    address public alice = makeAddr("alice");
    address public bob = makeAddr("bob");

    function setUp() public {
        wprim = new WPRIM();
        vm.deal(alice, 100 ether);
        vm.deal(bob, 50 ether);
    }

    function test_metadata() public view {
        assertEq(wprim.name(), "Wrapped PRIM");
        assertEq(wprim.symbol(), "WPRIM");
        assertEq(wprim.decimals(), 18);
    }

    function test_deposit() public {
        vm.prank(alice);
        wprim.deposit{value: 10 ether}();
        assertEq(wprim.balanceOf(alice), 10 ether);
        assertEq(wprim.totalSupply(), 10 ether);
    }

    function test_deposit_via_receive() public {
        vm.prank(alice);
        (bool ok,) = address(wprim).call{value: 5 ether}("");
        assertTrue(ok);
        assertEq(wprim.balanceOf(alice), 5 ether);
    }

    function test_withdraw() public {
        vm.startPrank(alice);
        wprim.deposit{value: 10 ether}();
        uint256 balBefore = alice.balance;
        wprim.withdraw(3 ether);
        assertEq(wprim.balanceOf(alice), 7 ether);
        assertEq(alice.balance, balBefore + 3 ether);
        vm.stopPrank();
    }

    function test_withdraw_insufficient() public {
        vm.startPrank(alice);
        wprim.deposit{value: 1 ether}();
        vm.expectRevert("WPRIM: insufficient balance");
        wprim.withdraw(2 ether);
        vm.stopPrank();
    }

    function test_transfer() public {
        vm.prank(alice);
        wprim.deposit{value: 10 ether}();

        vm.prank(alice);
        wprim.transfer(bob, 4 ether);
        assertEq(wprim.balanceOf(alice), 6 ether);
        assertEq(wprim.balanceOf(bob), 4 ether);
    }

    function test_approve_and_transferFrom() public {
        vm.prank(alice);
        wprim.deposit{value: 10 ether}();

        vm.prank(alice);
        wprim.approve(bob, 5 ether);
        assertEq(wprim.allowance(alice, bob), 5 ether);

        vm.prank(bob);
        wprim.transferFrom(alice, bob, 3 ether);
        assertEq(wprim.balanceOf(bob), 3 ether);
        assertEq(wprim.allowance(alice, bob), 2 ether);
    }

    function test_transferFrom_max_allowance() public {
        vm.prank(alice);
        wprim.deposit{value: 10 ether}();

        vm.prank(alice);
        wprim.approve(bob, type(uint256).max);

        vm.prank(bob);
        wprim.transferFrom(alice, bob, 5 ether);
        assertEq(wprim.allowance(alice, bob), type(uint256).max);
    }

    function test_transferFrom_insufficient_allowance() public {
        vm.prank(alice);
        wprim.deposit{value: 10 ether}();

        vm.prank(alice);
        wprim.approve(bob, 1 ether);

        vm.prank(bob);
        vm.expectRevert("WPRIM: insufficient allowance");
        wprim.transferFrom(alice, bob, 5 ether);
    }

    function testFuzz_deposit_withdraw(uint96 amount) public {
        vm.assume(amount > 0 && amount <= 100 ether);
        vm.deal(alice, uint256(amount));
        vm.startPrank(alice);
        wprim.deposit{value: amount}();
        assertEq(wprim.balanceOf(alice), amount);
        wprim.withdraw(amount);
        assertEq(wprim.balanceOf(alice), 0);
        vm.stopPrank();
    }
}
