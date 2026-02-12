# **First: Correct the invariant (important nuance)**

You said:

👉 *Invariant: No order reaches the matcher unless risk-approved.*

This is **almost correct**, but needs refinement for determinism.

### **Correct invariant**

**No order is ACCEPTED into the event log unless risk-approved at that sequence point.**

Why this matters:

* Risk checks happen **before sequencing**

* Matching happens **after sequencing**

* Risk engine never races with matching engine

Keep that locked in.

---

# **1️⃣ What the Risk Engine actually is**

The risk engine is **not**:

* A calculator

* A balance checker

* A liquidation bot

The risk engine is:

**A deterministic validator of future state viability under worst-case assumptions**

This is why most exchanges fail — they simulate *happy paths*.

---

# **2️⃣ Core responsibilities (expanded properly)**

Let’s rewrite your list with precision.

## **A. Position tracking (ground truth)**

The risk engine maintains **canonical positions per user**.

For options:

`Position {`  
  `instrument_id`  
  `side        // LONG or SHORT`  
  `quantity    // contracts (atomic)`  
  `avg_price`  
`}`

Important:

* LONG options → risk is capped

* SHORT options → risk is theoretically unbounded

👉 Risk engine must know **directionality**.

---

## **B. Exposure modeling (instrument-aware)**

Each instrument contributes exposure differently.

### **Example (call option)**

| Side | Risk profile |
| ----- | ----- |
| Long Call | Max loss \= premium paid |
| Short Call | Max loss \= ∞ (unbounded) |

### **Put option**

| Side | Risk profile |
| ----- | ----- |
| Long Put | Max loss \= premium |
| Short Put | Max loss \= strike × contract\_size |

Risk engine **does not care about price now** — it cares about **worst-case payoff**.

---

## **C. Margin calculation (core gate)**

Margin \= **capital locked to guarantee solvency**

Two broad models:

### **1️⃣ Fixed margin (v0, recommended)**

Simple, conservative, safe.

Examples:

* Short call → margin \= max\_loss\_estimate

* Short put → margin \= strike × quantity × multiplier

* Long options → margin \= premium (paid upfront)

This is how you should start.

---

### **2️⃣ Portfolio margin (SPAN-like) — later**

* Scenario-based loss simulation

* Correlation-aware

* Complex, dangerous if rushed

You **should not** start here.

---

## **D. Portfolio risk aggregation**

Users don’t have single positions — they have **portfolios**.

Risk engine must compute:

`Total Initial Margin`  
`Total Maintenance Margin`  
`Available Free Margin`

Example:

`equity = wallet_balance + unrealized_pnl`  
`free_margin = equity - initial_margin`

👉 OMS may show this, but **risk engine computes it**.

---

## **E. Liquidation eligibility**

Liquidation is **not an action**, it is a **state**.

A user is liquidatable if:

`equity < maintenance_margin`

Risk engine only declares:

“This account is liquidatable.”

It does **not** execute liquidation trades.

---

## **F. Exposure limits (exchange protection)**

Hard caps to prevent system death:

* Max contracts per instrument

* Max notional per user

* Max open positions

* Max short exposure

These are **absolute gates**, not margin-based.

---

# **3️⃣ Sequence-safe architecture (this is crucial)**

If you mess this up, everything breaks.

---

## **Event timeline (correct order)**

`User → OMS → Risk Engine → Sequencer → Matcher`

### **Step-by-step**

1. User submits order

2. OMS constructs order intent

3. OMS asks risk engine: “Is this survivable?”

4. Risk engine simulates post-order state

5. If approved → OMS submits to sequencer

6. Sequencer assigns sequence number

7. Matching engine executes deterministically

---

## **Why risk engine must be synchronous**

Risk engine **cannot** consume matcher events asynchronously for gating.

Instead:

* It maintains **its own mirrored state**

* Updated strictly from the same event log

This guarantees:

* Replay correctness

* Crash recovery

* No race conditions

---

# **4️⃣ Risk engine internal state**

Risk engine maintains:

### **A. Positions**

`positions[user_id][instrument_id] → Position`

### **B. Open orders (reserved margin)**

This is what people forget.

When an order is **accepted but not filled**, it already consumes risk.

`OpenOrderReservation {`  
  `order_id`  
  `worst_case_margin`  
`}`

Margin is reserved **before** execution.

---

### **C. Wallet balances**

Risk engine reads:

* Deposits

* Withdrawals

* Settlements

But **never modifies balances directly**.

---

# **5️⃣ Risk check algorithm (deterministic)**

This is the heart.

### **On new order intent:**

1. Clone user risk state (conceptually)

2. Apply hypothetical fill of **entire order**

3. Compute:

   * New positions

   * Worst-case loss

   * Required margin

4. Check:

   * free\_margin ≥ required\_margin

   * exposure limits not violated

5. Approve or reject

**Partial fills do NOT matter for approval.**

Worst-case assumes **full execution**.

---

## **Why worst-case fill assumption is mandatory**

Because:

* Partial fills are nondeterministic

* Liquidity can appear suddenly

* You cannot approve “hoping” it won’t fill

---

# **6️⃣ What happens after partial fills?**

Matching engine emits:

`TradeExecuted`  
`OrderPartiallyFilled`  
`OrderFilled`

Risk engine consumes these events **after sequencing** and:

* Converts reserved margin → actual margin

* Updates positions

* Releases unused reservation

---

# **7️⃣ Liquidation interaction (sequence-safe)**

Liquidation is triggered by **state**, not time.

### **Flow:**

1. Risk engine detects equity \< maintenance

2. Emits `LiquidationEligible` event

3. Liquidation engine submits forced orders via OMS

4. Those orders go through **same risk gates** (with override flag)

This preserves:

* Determinism

* Auditability

* Replayability

---

# **8️⃣ What the risk engine NEVER does**

Let this burn in:

❌ Match orders  
 ❌ Decide prices  
 ❌ Call matching engine  
 ❌ Mutate balances directly  
 ❌ Skip sequence rules

It is a **pure validator \+ state machine**.

---

# **9️⃣ Minimal v0 Risk Engine (what you should build first)**

You do **not** need SPAN to be safe initially.

### **v0 features (sufficient):**

* Atomic contract units

* Worst-case margin per instrument

* Full-fill assumption

* No portfolio offsets

* Fixed margin formulas

* Hard exposure caps

This already puts you ahead of 80% of crypto exchanges.

---

# **🔑 Mental model to lock in**

* Matching engine answers: *“What trades happen?”*

* Risk engine answers: *“Should this intent exist at all?”*

* OMS answers: *“What does the user want?”*

* Sequencer answers: *“In what order does reality happen?”*

#  **First principles: payoff vs margin**

**Payoff** \= what the option is worth at expiry  
 **PnL** \= payoff − premium  
 **Margin** \= capital locked *before expiry* to guarantee solvency under worst case

Margin is about **survivability**, not fairness.

---

## **Core variables (define once)**

Let:

* `S` \= underlying price at expiry

* `K` \= strike price

* `Q` \= number of contracts

* `C` \= contract\_size (in BTC / ETH units)

* `P` \= option premium (price per contract)

* Settlement currency \= USDT

Total notional exposure \= `Q × C`

---

# **2️⃣ Long options (easy case)**

## **✅ Long Call**

**Payoff**

`max(S − K, 0) × Q × C`

**Worst-case loss**

`premium paid = P × Q`

### **Margin rule (LONG CALL)**

`Initial Margin = P × Q`  
`Maintenance Margin = 0`

Why:

* Loss is capped at premium

* User already paid it

* No additional capital required

---

## **✅ Long Put**

**Payoff**

`max(K − S, 0) × Q × C`

**Worst-case loss**

`premium paid = P × Q`

### **Margin rule (LONG PUT)**

`Initial Margin = P × Q`  
`Maintenance Margin = 0`

Same logic. Long options are **pre-paid risk**.

👉 Long positions **never cause liquidation**.

---

# **3️⃣ Short options (this is where exchanges die)**

Short \= you wrote the option. You owe payoff if exercised.

---

## **❌ Short Call (most dangerous)**

**Payoff owed**

`max(S − K, 0) × Q × C`

**Worst-case loss**

`S → ∞ → loss → ∞`

This is *literally unbounded*.

So margin must be **conservative**.

---

## **❌ Short Put**

**Payoff owed**

`max(K − S, 0) × Q × C`

**Worst-case**

`S → 0`  
`loss = K × Q × C`

Bounded, but large.

---

# **4️⃣ Exact v0 margin formulas (what you should implement)**

These are **industry-standard conservative formulas**, simplified for safety.

---

## **🔴 Short Call — Initial Margin**

We cannot use “infinite”, so we approximate with **stress bounds**.

### **Recommended v0 formula**

`Initial Margin =`  
`Q × C × max(`  
  `α × S_current,`  
  `β × (S_current − K)`  
`)`

Where:

* `α` \= stress multiplier (e.g. 0.15 → 15%)

* `β` \= OTM sensitivity (e.g. 1.0)

### **Concrete example**

* BTC price \= 50,000

* Strike \= 55,000 (OTM)

* Q × C \= 0.01 BTC

* α \= 15%

`Margin = 0.01 × max(7500, 0) = 75 USDT`

If ITM:

`Margin = 0.01 × max(7500, 5000) = 75 USDT`

This ensures:

* Deep ITM calls require more margin

* OTM calls still require safety buffer

---

## **🔴 Short Call — Maintenance Margin**

`Maintenance Margin = 0.75 × Initial Margin`

Below this → liquidation eligible.

---

## **🔴 Short Put — Initial Margin**

Short puts are easier.

### **Formula**

`Initial Margin = Q × C × K`

This is the **absolute worst-case payout**.

Example:

* Strike \= 40,000

* Q × C \= 0.01 BTC

`Margin = 400 USDT`

No price feeds required.

---

## **🔴 Short Put — Maintenance Margin**

`Maintenance Margin = 0.75 × Initial Margin`

---

# **5️⃣ Why these formulas are correct (important intuition)**

### **Why not use current price for puts?**

Because worst-case is **S \= 0**, not today’s price.

### **Why use stress multipliers for calls?**

Because you need:

* Deterministic bounds

* Exchange-wide consistency

* No dependency on vol models initially

This is **deliberately conservative**.

---

# **6️⃣ Reserved margin vs actual margin (critical distinction)**

When an order is **placed** but not filled:

`Reserved Margin = margin_required_for_full_fill`

When fills happen:

* Reserved margin → actual margin

* Excess is released

This prevents **race-condition insolvency**.

---

# **7️⃣ Risk Engine logic (pseudocode)**

`fn check_order(order, user_state) -> Result<()> {`  
    `let hypothetical_state = user_state.clone();`  
    `hypothetical_state.apply_full_fill(order);`

    `let required_margin = compute_margin(hypothetical_state);`  
    `let equity = wallet_balance + unrealized_pnl;`

    `if equity < required_margin {`  
        `reject();`  
    `}`

    `approve();`  
`}`

No price guessing. No partial optimism.

---

# **8️⃣ Summary table (lock this in)**

| Position | Max Loss | Initial Margin |
| ----- | ----- | ----- |
| Long Call | Premium | Premium |
| Long Put | Premium | Premium |
| Short Call | Unbounded | Stress-based |
| Short Put | Strike × size | Full notional |

This table alone explains **80% of risk logic**.

# **Maintenance Margin & Liquidation Sequencing**

*(This is a state machine, not a cron job)*

## **First: definitions (very strict)**

### **Equity**

`Equity = Wallet Balance + Unrealized PnL`

### **Initial Margin**

Capital required to **open or increase** risk.

### **Maintenance Margin**

Capital required to **keep positions alive**.

Rule (industry standard):

`Maintenance Margin = γ × Initial Margin`

Where γ ≈ 0.75 (you can tune later).

---

## **Liquidation condition (single line)**

`If Equity < Maintenance Margin → Account is LIQUIDATABLE`

Not liquidated yet.  
 Just **eligible**.

This distinction matters a LOT.

---

## **Sequencing rule (non-negotiable)**

**Liquidation eligibility is evaluated ONLY after sequenced events.**

Why?

* Determinism

* Replay safety

* No race between matcher and risk engine

---

## **Liquidation lifecycle (state machine)**

`HEALTHY`  
  `↓ (equity < maintenance)`  
`LIQUIDATABLE`  
  `↓ (liquidation engine picks it up)`  
`LIQUIDATING`  
  `↓ (positions closed / transferred)`  
`RESOLVED`

Risk engine only:

* Declares `LIQUIDATABLE`

* Clears flag once resolved

It does NOT trade.

---

# **2️⃣ Liquidation Engine Mechanics**

*(This is a separate system, intentionally)*

There are **two canonical models**:

---

## **🔴 Model A: Taker Liquidation (simplest, v0 recommended)**

### **How it works**

Risk engine emits:

 `LiquidationEligible { user_id }`

1.   
2. Liquidation engine submits **market orders** on behalf of the user

3. Orders go through OMS → Sequencer → Matcher

4. Positions close at best available price

5. Losses deducted from margin

6. Any remaining balance returned to user

### **Pros**

* Simple

* Fast

* Deterministic

* Easy to reason about

### **Cons**

* Slippage risk

* Can cascade during volatility

This is **fine for v0** if margin is conservative.

---

## **🔵 Model B: Auction Liquidation (advanced, safer)**

Used by Deribit, CME, advanced venues.

### **How it works**

1. Position is frozen

2. Exchange opens an **auction window** (e.g. 100ms–1s)

3. Market makers submit bids to take over position

4. Best bid wins

5. Position transferred atomically

### **Pros**

* Reduced slippage

* Better price discovery

* Less market impact

### **Cons**

* Complex

* Latency-sensitive

* Harder to implement deterministically

❗ Do **not** start here.

---

## **Recommendation (clear)**

👉 **Start with taker liquidation**  
 Add auctions only after volume \+ stability.

---

## **Partial liquidation (important nuance)**

Never liquidate “everything always”.

Algorithm:

1. Close riskiest positions first

2. Recompute equity

3. Stop once equity ≥ maintenance margin

This prevents unnecessary damage.

---

# **3️⃣ OMS \+ Risk \+ Matcher sequencing during liquidation**

This is where people mess up.

### **Liquidation orders are NOT special**

They:

* Go through OMS

* Are sequenced

* Are matched normally

The ONLY difference:

* They bypass *user intent*

* They may bypass some risk checks (with a flag)

But they still respect:

* Order book rules

* Price-time priority

* Determinism

---

# **4️⃣ Replay & Crash Recovery (this is critical)**

If your system can’t replay from genesis, it is **not an exchange**.

---

## **Single source of truth**

**The event log is the only source of truth.**

Everything else is derived.

---

## **What is logged (append-only)**

Examples:

`OrderAccepted`  
`OrderRejected`  
`OrderMatched`  
`TradeExecuted`  
`PositionUpdated`  
`MarginReserved`  
`MarginReleased`  
`LiquidationEligible`  
`LiquidationOrderPlaced`  
`SettlementCompleted`

Each event has:

* Sequence number

* Deterministic payload

* No side effects

---

## **Replay model**

On restart:

1. Clear all in-memory state

2. Replay events in sequence order

3. Rebuild:

   * Order books

   * Positions

   * Margins

   * Wallet balances

4. Resume from last sequence

If replay ≠ live state → **bug**

---

## **Why this works**

* OMS doesn’t “remember” orders — events do

* Risk engine doesn’t “store truth” — it derives it

* Matcher doesn’t “own trades” — it emits them

Everything is **purely functional over time**.

---

# **5️⃣ Crash scenarios (let’s test your design)**

### **Scenario A: Crash mid-liquidation**

* Some orders placed, some not

* On replay:

  * Same liquidation eligibility event appears

  * Same liquidation orders re-submitted

  * Same matches occur  
     ✔ Deterministic

---

### **Scenario B: Crash after trade, before margin update**

* Trade event exists

* On replay:

  * Risk engine consumes trade

  * Margin updated correctly  
     ✔ Safe

---

### **Scenario C: OMS accepted order, matcher didn’t run yet**

* OrderAccepted event exists

* On replay:

  * Matcher consumes it

  * Same matching occurs  
     ✔ Safe

---

# **6️⃣ Invariants to burn into your brain**

These are **laws**, not suggestions:

1. Risk engine never places trades

2. Liquidation engine never mutates balances

3. Matcher never checks margin

4. OMS never changes positions

5. Everything is replayable

6. Same events → same state (always)

Break one → exchange dies eventually.

---

# **7️⃣ Mental compression (final)**

* **Maintenance margin** \= survival threshold

* **Liquidation** \= forced intent, not forced execution

* **Auctions** \= optimization, not necessity

* **Replayability** \= legitimacy

