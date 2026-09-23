---
name: ctf-web3
description: Blockchain security analysis for CTF challenges — authorized educational environment covering smart contract analysis, DeFi vulnerability verification, and vulnerability assessment patterns.
---

# CTF Blockchain Security Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Setup & Tooling

### Foundry (Preferred)
```bash
forge init ctf && cd ctf
forge install foundry-rs/forge-std
forge test --match-test <name> -vvvv
```

### Hardhat
```bash
npx hardhat init
npx hardhat test --network hardhat
```

### Key Dependencies
```bash
npm i @openzeppelin/contracts @uniswap/v2-core ethers
```

## Solidity Vulnerability Patterns

### Reentrancy (Classic)
```solidity
// VULNERABLE
function withdraw() external {
    uint bal = balances[msg.sender];
    (bool sent,) = msg.sender.call{value: bal}("");
    require(sent);
    balances[msg.sender] = 0;
}

// FIX: Checks-Effects-Interactions
function withdraw() external {
    uint bal = balances[msg.sender];
    balances[msg.sender] = 0;  // effect before interaction
    (bool sent,) = msg.sender.call{value: bal}("");
    require(sent);
}
```

### Cross-Function Reentrancy
```solidity
// Attacker calls multiple functions sharing state
function attack() external {
    vault.withdraw();  // reenters before state update
    vault.somethingElse();  // reads stale balances
}
```

### Delegatecall Injection
```solidity
// If attacker controls target address in delegatecall
function execute(address _target, bytes memory _data) external {
    (bool success,) = _target.delegatecall(_data);
}
// Attacker points to malicious contract that writes storage slots
```

### Access Control Bypass
```solidity
// tx.origin vs msg.sender
require(msg.sender == owner);  // correct
require(tx.origin == owner);   // vulnerable to phishing

// Missing modifier on function
function mint(address to, uint amount) external { }  // public, no check
```

### Flash Loan Analysis
```solidity
// Uniswap V2 flash loan
function flashLoan(uint amount) external {
    address[] memory path = new address[](2);
    path[0] = tokenA;
    path[1] = tokenB;
    IUniswapV2Router(UniswapV2Router).swapExactTokensForTokens(
        amount, 0, path, address(this), block.timestamp
    );
    // Manipulate price, exploit contract, repay
}
```

### Integer Overflow/Underflow (pre-0.8)
```solidity
// Solidity <0.8 - use SafeMath
uint8 x = 255;
x += 1;  // wraps to 0
```

### Price Oracle Analysis
```solidity
// Single DEX price feed = vulnerable
// Use TWAP or Chainlink oracle
function getPrice() internal view returns (uint) {
    return pair.getReserves()[0] * 1e18 / pair.getReserves()[1];
}
```

## Ethernaut Solutions

### Level 1-5 Quick Solves
```solidity
// 1-Force: selfdestruct to send ETH
contract Force { function attack(address target) external payable { selfdestruct(payable(target)); } }
// 2-Vault: storage slot 1
bytes32 pw = vm.load(address(vault), bytes32(uint(1)));
// 3-Token: overflow uint256(0) - 1
// 4-Delegatecall: call with matching msg.value and function selector
// 5-Telephone: block.timestamp != tx.timestamp
```

## Common CTF Patterns

### Storage Layout
```bash
# Read private storage slot
cast storage <addr> <slot> --rpc-url <rpc>
```

### Signature Replay
```solidity
// Check nonce + chainId
bytes32 structHash = keccak256(abi.encode(nonce, to, amount));
bytes32 hash = keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR, structHash));
```

### Frontrunning Simulation
```solidity
// Mempool: see pending tx, front-run with higher gas
// Use Flashbots to hide tx
```

## CTF References
- **Ethernaut**: OpenZeppelin's 30+ challenge set (standard reference)
- **DeFi Security Summit 2024**: Price oracle manipulation challenges
- **Paradigm CTF 2024**: Advanced flash loan + MEV challenges
- **DownUnderCTF 2024**: Solidity reentrancy with twist
- **Blockchain CTF 2024**: Cross-chain bridge analysis
- **EclipseCTF 2025**: SVM/EVM hybrid vulnerabilities
- **corCTF 2025**: L2 rollup state transition analysis
- **HITCON CTF 2025**: MEV bot analysis

## Speed Metrics
| Metric | Target |
|--------|--------|
| Contract compilation | <15s |
| Vulnerability ID | <60s |
| Analysis deploy | <120s |
| Ethernaut solve | <90s |
| CTF chain interaction | <30s |
