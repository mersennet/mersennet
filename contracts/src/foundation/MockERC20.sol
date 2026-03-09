// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MockERC20
/// @notice Testnet mock token with public mint — use for USDC, USDT, DAI, etc.
/// @dev Anyone can mint up to 10,000 tokens per call (testnet only)
contract MockERC20 {
    string public name;
    string public symbol;
    uint8  public immutable decimals;

    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    uint256 public constant MAX_MINT = 10_000;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    constructor(string memory _name, string memory _symbol, uint8 _decimals) {
        name = _name;
        symbol = _symbol;
        decimals = _decimals;
    }

    /// @notice Mint testnet tokens — anyone can call, max 10,000 per tx
    function mint(address to, uint256 amount) external {
        uint256 scaled = amount / (10 ** decimals);
        require(scaled <= MAX_MINT, "MockERC20: max 10,000 tokens per mint");
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    /// @notice Convenience: mint max amount to caller
    function faucet() external {
        uint256 amount = MAX_MINT * (10 ** decimals);
        totalSupply += amount;
        balanceOf[msg.sender] += amount;
        emit Transfer(address(0), msg.sender, amount);
    }

    function approve(address spender, uint256 value) external returns (bool) {
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
        return true;
    }

    function transfer(address to, uint256 value) external returns (bool) {
        return _transfer(msg.sender, to, value);
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        if (allowance[from][msg.sender] != type(uint256).max) {
            require(allowance[from][msg.sender] >= value, "MockERC20: insufficient allowance");
            allowance[from][msg.sender] -= value;
        }
        return _transfer(from, to, value);
    }

    function _transfer(address from, address to, uint256 value) internal returns (bool) {
        require(balanceOf[from] >= value, "MockERC20: insufficient balance");
        balanceOf[from] -= value;
        balanceOf[to] += value;
        emit Transfer(from, to, value);
        return true;
    }
}
