---
name: web3-audit
description: Smart contract security audit — 10 DeFi bug classes (accounting desync, access control, incomplete path, off-by-one, oracle, ERC4626, reentrancy, flash loan, signature replay, proxy), pre-dive kill signals (TVL < $500K etc), Foundry PoC template, grep patterns for each class, and real Immunefi paid examples. Use for any Solidity/Rust contract audit or when deciding whether a DeFi target is worth hunting.
---

# WEB3 SMART CONTRACT AUDIT

10 bug classes. Pre-dive kill signals. Foundry PoC template. Real paid examples.

---

## PRE-DIVE KILL SIGNALS (check BEFORE any code review)

> ZKsync lesson: $322M TVL + OZ audit + 750K LOC + 5 sessions = 0 findings. Large well-audited bridges are extremely hard.

1. **TVL < $500K** → max payout capped too low for effort
2. **2+ top-tier audits** (Halborn, ToB, Cyfrin, OpenZeppelin) on simple protocol → bugs already found
3. **Protocol < 500 lines, single A→B→C flow** → minimal attack surface
4. **Formula**: `max_realistic_payout = min(10% × TVL, program_cap)` — if < $10K, skip

**Target scoring (go if >= 6/10):**
- TVL > $10M: +2
- Immunefi program with Critical >= $50K: +2
- No top-tier audit on current version: +2
- < 30 days since deploy: +1
- Protocol you've hunted before: +1
- Source code + natspec comments: +1
- Upgradeable proxies: +1

---

## THE ONE RULE

> "Read ALL sibling functions. If `vote()` has a modifier, check `poke()`, `reset()`, `harvest()`. The missing modifier on the sibling IS the bug."

This single rule explains 19% of all Critical findings.

---

## 1. ACCOUNTING STATE DESYNCHRONIZATION

> #1 Critical bug class — 28% of all Criticals on Immunefi.

**Root Cause:** Two state variables supposed to stay in sync. One code path updates A but forgets B.

```
Real Value = A - B
If A updated but B isn't → Real Value appears larger → phantom value
```

### Variants

**Variant 1: Phantom Yield**
```solidity
function startUnstake(uint256 amount) external {
    totalSupply -= amount;  // decremented BEFORE transfer
    // aToken.balanceOf(this) still reflects old value
    // yieldAmount = aToken.balanceOf - totalSupply = phantom yield
}
```

**Variant 2: Fast Path Skips State Update**
```solidity
function claimRedemption(uint256 tokenId) external {
    if (transmuter.balance >= amount) {
        transmuter.transfer(user, amount);
        _burn(tokenId);
        return;  // EARLY RETURN — state vars never updated
    }
    // Slow path: updates all state vars correctly
}
```

**Variant 3: Update Happens in Wrong Order**
```solidity
function deposit(uint256 amount) external {
    _shares = (amount * totalShares) / totalAssets;  // calculated BEFORE deposit
    totalAssets += amount;  // assets added AFTER → wrong rate
}
```

### Grep Patterns
```bash
grep -rn "totalSupply\|totalShares\|totalAssets\|totalDebt" contracts/
grep -rn "\breturn\b" contracts/ -B3 | grep -B3 "if\b"
```

---

## 2. ACCESS CONTROL

> #2 Critical — 19% of Criticals. $953M lost in 2024 alone.

### Variants

**Missing Modifier on Sibling Function:**
```solidity
function vote(uint256 tokenId) external onlyNewEpoch(tokenId) {  // guarded
function reset(uint256 tokenId) external onlyNewEpoch(tokenId) {  // guarded
function poke(uint256 tokenId) external {  // NO GUARD → infinite FLUX inflation
}
```

**Wrong Check (Existence vs Ownership):**
```solidity
function split(uint256 tokenId, uint256 amount) external {
    _requireOwned(tokenId);  // checks if token EXISTS, not if caller OWNS it
    _burn(tokenId);
    _mint(msg.sender, amount);  // attacker steals tokens they don't own
}
```

**Silent Modifier (if vs require):**
```solidity
// VULNERABLE — non-admin silently gets through:
modifier onlyAdmin() {
    if (msg.sender == admin) { _; }  // body only for admin, non-admin doesn't revert
}
// CORRECT: require(msg.sender == admin, "Not admin"); _;
```

**Uninitialized Proxy:**
```solidity
function initialize(address _owner) public {  // MISSING: initializer modifier
    owner = _owner;  // anyone can call → become owner
}
```

### Real Paid Examples

| Protocol | Payout | Bug |
|----------|--------|-----|
| Wormhole | $10M | Uninitialized UUPS proxy |
| Parity | $150M frozen | No access control on initWallet() |

---

## 3. INCOMPLETE CODE PATH

> #3 Critical — 17% of Criticals.

### The Function Family Comparison Test
```
1. List all state changes in function A (deposit/place/create)
2. List all state changes in function B (withdraw/update/cancel)
3. For each state change in A: does B have the corresponding reverse?
4. For each token transfer in A: does B have the corresponding refund?
```

### Variant 1: Update Function Missing Refund
```solidity
function place_order(OrderInput calldata order) external {
    token.safeTransferFrom(msg.sender, address(this), order.price);
    orders[orderId] = order;
}
function update_order(OrderInput calldata updatedOrder) external {
    // BUG: NO REFUND for sell orders when price decreases
    orders[orderId] = updatedOrder;
}
```

### Variant 2: mint() Bypasses Check That deposit() Has
```solidity
function deposit(uint256 assets, address receiver) public override {
    shares = _deposit(assets, receiver);  // includes receipt validation
}
function mint(uint256 shares, address receiver) public override {
    assets = convertToAssets(shares);
    _mint(receiver, shares);  // MISSING: validation → mints without receiving assets
}
```

---

## 4. OFF-BY-ONE & BOUNDARY CONDITIONS

> #4 High — 22% of Highs. Single character change. Massive impact.

**Mental Test:** For every `if (A > B)`: "What happens when A == B?"

### 6 Boundary Locations to Check
1. Period/Epoch boundaries: `>` vs `>=`
2. Time-based locks: `block.timestamp == deadline`
3. Loop break conditions
4. Array index boundaries: `i <= array.length`
5. Amount/balance boundaries
6. Rounding/precision

---

## 5. ORACLE / PRICE MANIPULATION

> 12% of all reports. Largest individual payouts. $117M Mango, $70M Curve.

### Bug A: Missing Staleness Check
```solidity
(, int256 price,,,) = priceFeed.latestRoundData();
return uint256(price);  // If Chainlink node goes down, stale price returned indefinitely
// CORRECT:
(, int256 price,, uint256 updatedAt,) = priceFeed.latestRoundData();
require(block.timestamp - updatedAt <= MAX_PRICE_AGE, "Stale price");
```

### Bug B: TWAP Too Short
```solidity
// VULNERABLE: 60-second TWAP — flash loan can shift price for entire window
secondsAgos[0] = 60; secondsAgos[1] = 0;
// CORRECT: 1800s minimum TWAP (30 min)
```

### Bug C: Single-Source Oracle
```solidity
uint price = getUniswapSpotPrice(token);  // flash loan manipulatable
// CORRECT: Chainlink primary, Uniswap TWAP as fallback
```

---

## 6. ERC4626 VAULT ATTACKS

### Exchange Rate Manipulation (near-empty vault)
```solidity
// 1. Attacker deposits 1 wei → gets 1 share
// 2. Attacker donates large amount directly (transfer, not deposit)
// 3. Exchange rate: 1 share = (1 + donation) assets
// 4. Victim deposits → rounds down to 0 shares → free donation
// CORRECT: virtual shares (OpenZeppelin v4.9+)
function _decimalsOffset() internal view virtual override returns (uint8) {
    return 9;  // add 1e9 virtual shares + assets
}
```

---

## 7. REENTRANCY

```solidity
// VULNERABLE (effects after interaction):
function withdraw(uint256 amount) external {
    require(balances[msg.sender] >= amount);
    (bool success,) = msg.sender.call{value: amount}("");  // INTERACTION first
    balances[msg.sender] -= amount;  // EFFECT after → reentrancy window
}
// CORRECT (CEI):
function withdraw(uint256 amount) external {
    require(balances[msg.sender] >= amount);
    balances[msg.sender] -= amount;  // EFFECT
    (bool success,) = msg.sender.call{value: amount}("");  // INTERACTION last
}
```

### Variants
- **Single-function**: re-enters same function before state updated
- **Cross-function**: re-enters a sibling function with stale state
- **Cross-contract**: re-enters via callback to another protocol
- **Read-only**: re-enters a view function that returns stale data

---

## 8. FLASH LOAN ATTACKS

**Attack flow:**
```
1. Borrow $100M from Aave flash loan
2. Dump token in Uniswap pool → crash spot price
3. Protocol reads Uniswap spot → undercollateralized loans accepted
4. Borrow max against cheap collateral
5. Repay flash loan, keep profits
```

**What to look for:**
```bash
grep -rn "getReserves\|getAmountsOut\|slot0\b" contracts/ -A5
# spot price from reserves = manipulatable with flash loan
```

---

## 9. SIGNATURE REPLAY

```solidity
// VULNERABLE — missing nonce:
function permit(...) external {
    bytes32 hash = keccak256(abi.encodePacked(owner, spender, value, deadline));
    // MISSING: nonce not included → same signature usable multiple times
}

// VULNERABLE — missing chain ID:
bytes32 hash = keccak256(abi.encodePacked(params));
// MISSING: block.chainid not in hash → works on any chain
```

---

## 10. PROXY / UPGRADE ISSUES

### Storage Collision
```
Implementation and proxy share storage layout
Proxy slot 0: _owner
Implementation slot 0: _initialized
→ writing to _initialized overwrites _owner
```

### Uninitialized Implementation
```solidity
// If implementation can be initialized directly → anyone becomes owner
// Fix: constructor() { _disableInitializers(); }
```

---

## FOUNDRY POC TEMPLATE

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "forge-std/Test.sol";
import "../src/VulnerableContract.sol";

contract ExploitTest is Test {
    VulnerableContract target;
    AttackerContract attacker;

    function setUp() public {
        target = new VulnerableContract();
        attacker = new AttackerContract(address(target));
    }

    function test_exploit() public {
        // 1. Setup
        deal(address(this), 100 ether);

        // 2. Attack
        uint256 beforeBalance = address(this).balance;
        attacker.attack{value: 100 ether}();

        // 3. Verify
        uint256 afterBalance = address(this).balance;
        assertGt(afterBalance, beforeBalance, "Exploit did not profit");
        emit log_named_uint("Profit (wei)", afterBalance - beforeBalance);
    }
}

contract AttackerContract {
    VulnerableContract target;

    constructor(VulnerableContract _target) {
        target = _target;
    }

    function attack() external payable {
        // Exploit logic here
    }

    receive() external payable {}
}
```

**Run with:**
```bash
forge test --match-test test_exploit -vvvv
```
