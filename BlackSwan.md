# **PART I — TEST STRATEGY PHILOSOPHY (IMPORTANT)**

Before scenarios, one core truth:

**You don’t test exchanges by unit tests.**  
 **You test them by adversarial simulations.**

Every test should try to break **invariants**, not features.

### **Core invariants recap**

These must *never* break, even if the UI is down, nodes die, or prices go insane.

1. **No negative balances**

2. **No trade without risk approval**

3. **Matching is deterministic**

4. **State is derivable from events**

5. **Losses never exceed collateral \+ insurance**

6. **Liquidations are monotonic** (cannot improve user position)

All tests revolve around *trying to violate these*.

---

# **PART II — BLACK-SWAN SIMULATION (MINUTE-BY-MINUTE)**

Let’s simulate a **realistic crypto options black swan**.

### **Scenario setup**

* Underlying: BTC

* Index price pre-event: **$60,000**

* Volatility surface: calm (IV \~ 55%)

* Users:

  * Retail longs

  * Market makers short gamma

  * One whale massively short calls

* Collateral: USDT

* Margin system: Portfolio margin \+ Greeks

---

## **Minute 0 — Normal Market**

Everything fine.

* Mark price \= fair BS price

* Order book tight

* Margin usage \~ 30–50%

* Insurance fund healthy

---

## **Minute 1 — Shock Trigger**

News hits:

“Major BTC exchange insolvent. Withdrawals frozen.”

### **Immediate effects**

* Index sources diverge

* Spot liquidity thins

* Volatility explodes

#### **System behavior**

* **Index price**: still computed from trusted basket

* **Mark price**: uses smoothing (EMA / clamp)

* **Trading continues**

Invariant protected:

Market data ≠ source of truth

---

## **Minute 2 — Volatility Spike**

* IV jumps from 55% → 180%

* Call premiums explode

* Short call sellers hemorrhage margin

### **Risk Engine behavior**

* Greeks recomputed

* Margin requirements increase

* **Orders from under-margined accounts are blocked**

Key rule:

Margin recalculation is **reactive**, not predictive.

No forced liquidation yet — just gating.

---

## **Minute 3 — First Liquidations Trigger**

Some accounts breach **maintenance margin**.

### **Liquidation engine activates**

For each account:

1. Freeze OMS access

2. Snapshot portfolio

3. Compute liquidation price bands

4. Start liquidation sequence

#### **Priority**

1. Reduce risk fastest

2. Minimize market impact

3. Protect insurance fund

---

## **Minute 4 — Liquidity Collapse**

Order books thin out.

Two things happen:

* Liquidation limit orders don’t fill

* Mark price diverges from last trade

### **Liquidation algorithm switches mode**

`If limit liquidation fails for T seconds:`  
    `→ switch to aggressive taker mode`

Safeguards:

* Slippage caps

* Kill if price crosses bankruptcy price

---

## **Minute 5 — Whale Goes Bankrupt**

Big short-call whale:

* Equity → 0

* Still has open positions

### **Bankruptcy handling**

* Position closed at worst available price

* Residual loss \= absorbed by insurance fund

Invariant:

User loss capped at collateral  
 System loss absorbed centrally

---

## **Minute 6 — Insurance Fund Drawdown**

Insurance fund decreases.

If fund \< threshold:

* Increase margin requirements globally

* Widen liquidation aggressiveness

* Tighten OMS gating

This is **auto-deleveraging prevention**.

---

## **Minute 7 — Cascade Risk**

Other users are close to liquidation.

Critical decision point:

* **Circuit breaker or continue?**

---

## **Minute 8 — Circuit Breaker Triggers**

Conditions met:

* Index volatility \> X

* Insurance drawdown rate \> Y

* Liquidation queue backlog \> Z

### **Actions**

* New order placement paused

* Cancellations allowed

* Liquidations continue

* Market data continues

This prevents **panic amplification**.

---

## **Minute 9–15 — Stabilization**

* Liquidations finish

* Volatility settles

* Circuit breaker lifted gradually

---

## **Minute 16 — Post-Mortem Mode**

System emits:

* Event logs

* Risk snapshots

* Liquidation reports

Used for:

* Replay

* Legal audit

* Regulator defense

---

# **PART III — WORST-CASE TEST CASES (MUST IMPLEMENT)**

These are **non-negotiable**.

---

## **1\. Replay Consistency Test**

Kill everything mid-liquidation.

### **Test**

* Stop OMS

* Stop Risk

* Stop Matcher

* Replay from event log

### **Must guarantee**

* Same positions

* Same balances

* Same liquidations

If not → **system invalid**

---

## **2\. Price Feed Attack**

### **Attack**

* One index source spikes \+500%

* Others normal

### **Defense**

* Median / trimmed mean

* Staleness rejection

* Circuit breaker

---

## **3\. Self-Trade / Wash Attack**

### **Attack**

* User trades with self to move mark price

### **Defense**

* Self-trade prevention

* Mark price ≠ last trade

* Liquidity-weighted price

---

## **4\. OMS Flood Attack**

### **Attack**

* Millions of cancel/replace orders

### **Defense**

* Rate limits

* Sequence gating

* Deterministic ordering

---

## **5\. Margin Oracle Lag**

### **Attack**

* Greeks not updating fast enough

### **Defense**

* Conservative margin floors

* Volatility clamps

* Fallback stress margin

---

## **6\. Liquidation Queue Explosion**

### **Attack**

* Thousands of accounts liquidated at once

### **Defense**

* Sharded liquidation workers

* Priority by risk exposure

* Global liquidation throttle

---

## **7\. Negative Balance Attempt**

### **Attack**

* Withdraw during liquidation

* Race OMS vs wallet

### **Defense**

* Balance locks

* State machine transitions

* Idempotent debits

---

# **PART IV — HOW EXCHANGES ACTUALLY DIE (REAL WORLD)**

Let’s be blunt.

### **❌ Not because of math**

### **❌ Not because of matching**

### **❌ Not because of pricing models**

They die because of:

---

## **1\. Cross-System Race Conditions**

OMS accepts order  
 Risk recalculates later  
 → under-collateralized trade sneaks in

**Fix**: Risk gates OMS synchronously

---

## **2\. Mark Price Abuse**

Using last trade as mark price.

**Fix**: Mark price must be model-based \+ smoothed

---

## **3\. Insurance Fund Mis-sizing**

Assuming “normal volatility”.

**Fix**: Stress scenarios × 10

---

## **4\. Partial Liquidations Done Wrong**

Reducing position but increasing gamma risk.

**Fix**: Liquidate by **risk contribution**, not size

---

## **5\. No Global Kill Switch**

Trying to “let the market decide”.

**Fix**: Centralized emergency authority (code-driven)

---

# **PART V — BLACK-SWAN DESIGN PRINCIPLES (MEMORIZE THESE)**

1. **Markets are adversarial**

2. **Users will try to bankrupt you**

3. **Volatility clusters**

4. **Liquidity disappears first**

5. **Everything fails at once**

6. **Determinism \> cleverness**

7. **Slow liquidation is safer than fast chaos**

