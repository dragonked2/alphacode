---
name: economic-attacks
description: Economic modeling — flash-loan capital, profit equations, manipulation cost, liquidation games. Load before calling anything an exploit.
---

# ECONOMIC ATTACKS

## Mandatory equation

```
netProfit = attackerGain - attackerCosts - requiredRepayment
```

Itemize: attacker capital, temporary (flash) capital and fees,
liquidity required vs available, slippage, gas, protocol loss,
attacker gain, repayment. Reject `netProfit <= 0` and attacks needing
liquidity that does not exist.

## Flash-loan pattern (capital, not bug)

```
flash liquidity → manipulation → protocol interaction →
extraction → restoration → repayment → profit
```

Ask: does temporary capital let the attacker hold a manipulated variable
long enough to violate an invariant? Atomicity cuts both ways — the
protocol interaction and the restoration happen in one transaction, so
the vulnerable window must be inside it.

## Lenses

- Liquidation games: discount vs bonus, MEV competition, dust positions.
- Oracle lag arbitrage between markets.
- Governance rented per block vs proposal value.
- Fee/reward farming exceeding intended emission rate.

## Rule

Economics without numbers is narrative. Every economic finding ships
with the arithmetic or it ships as a hypothesis, never as confirmed.
