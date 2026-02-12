# **What the Wallet System *is* (and is NOT)**

**Wallet ≠ Balance**  
 **Wallet \= authoritative collateral ledger**

The wallet system is the **only place where money exists**.

Everything else:

* OMS → intents

* Matching Engine → trades

* Risk Engine → permissions

* Clearing → state derivation

…all *reference* the wallet but **never mutate it directly**.

---

## **Wallet System Responsibilities**

**It owns:**

* Deposits

* Withdrawals

* Free balance

* Locked balance (margin / orders)

* Insurance fund

* Virtual wallets (test / simulation)

**It does NOT:**

* Calculate margin

* Track positions

* Decide liquidation

* Match orders

👉 **Invariant:**

Wallet state changes only via **Clearing events**, never via OMS or Matcher.

---

# **2️⃣ Core Wallet Data Model**

### **Account → Wallet → Balances**

`Wallet {`  
  `account_id`  
  `asset: USDT | BTC | ETH | ...`  
  `free_balance`  
  `locked_balance`  
  `total_balance = free + locked`  
  `version`  
`}`

**Important:**

* `total_balance` is **derived**, not stored

* `version` increments on every mutation (for replay & safety)

---

## **Multi-Asset Reality**

Most options exchanges use:

* **Single collateral** (USDT or USD)

* **Underlying asset** (BTC) is *not* collateral

So you’ll typically have:

`Wallet(USDT)  ← margin, PnL, liquidation`  
`Wallet(BTC)   ← spot only (optional)`

---

# **3️⃣ Locked vs Free Balance (CRITICAL)**

### **Free Balance**

* Available for:

  * New orders

  * Withdrawals

  * Fee payments

### **Locked Balance**

* Reserved for:

  * Open orders

  * Margin for positions

  * Liquidation buffers

`free_balance >= 0`  
`locked_balance >= 0`

👉 **Invariant:**

free\_balance \+ locked\_balance \= total\_balance  
 locked\_balance can NEVER be spent

---

# **4️⃣ How Locking Works (OMS → Risk → Wallet)**

### **When user places an order**

1. OMS receives intent

2. Risk Engine simulates:

   * Worst-case margin requirement

Risk Engine returns:

 `required_margin = X`

3. 

Wallet system:

 `free -= X`  
`locked += X`

4.   
5. Order allowed to reach matcher

**Key rule:**  
 Wallet locking happens **before** matching.

---

# **5️⃣ Unlocking Scenarios**

Locked funds are released when:

### **1\. Order Cancelled**

* Unlock unused margin

### **2\. Order Filled**

* Locked margin moves to **position margin**

* Excess margin unlocked

### **3\. Order Expired**

* Same as cancel

### **4\. Liquidation**

* Locked funds are seized

All of this happens via **Clearing events**, not directly.

---

# **6️⃣ Wallet Mutations Are Event-Driven**

Wallet does **not** mutate directly.

Instead:

`Event → Apply → New Wallet State`

### **Wallet Events**

`enum WalletEvent {`  
  `Deposit,`  
  `Withdrawal,`  
  `Lock,`  
  `Unlock,`  
  `Debit,`  
  `Credit,`  
  `InsuranceTransfer,`  
`}`

Each event is:

* Append-only

* Sequenced

* Replayable

👉 **Invariant:**

Wallet state \= fold(events)

---

# **7️⃣ Deposits & Withdrawals**

### **Deposits**

* External system credits wallet

* Instant free balance increase

`free += deposit_amount`

### **Withdrawals**

* Require:

  * free\_balance ≥ withdrawal

  * no pending locks

`free -= withdrawal_amount`

**Never touch locked balance.**

---

# **8️⃣ Insurance Fund (Later, But Design Now)**

Insurance fund is just:

`Wallet {`  
  `account_id = INSURANCE`  
  `asset = USDT`  
`}`

Used when:

* Liquidation deficit

* Socialized losses (worst case)

Transferred via:

`WalletEvent::InsuranceTransfer`

---

# **9️⃣ Virtual Wallets (EXTREMELY IMPORTANT)**

This is where your system becomes *elite*.

### **Why Virtual Wallets Exist**

You want to:

* Simulate strategies

* Test liquidation logic

* Replay crashes

* Run backtests

Without touching real money.

---

## **Virtual Wallet Architecture**

Same code.  
 Same engines.  
 Different namespace.

`Environment {`  
  `PROD`  
  `SANDBOX`  
  `SIMULATION`  
`}`

Each environment has:

* Separate wallet ledger

* Separate event stream

* Same logic

---

### **Virtual Deposits**

In sandbox:

`Deposit 1,000,000 USDT`

No chain interaction.  
 Just a ledger event.

---

### **Virtual Trades**

Matching Engine emits:

`TradeEvent`

Clearing consumes it → updates:

* Positions

* Margins

* Wallets

**Identical flow to production.**

---

# **🔟 Wallet \+ Clearing Interaction (VERY IMPORTANT)**

Wallet does **nothing on trades directly**.

Flow:

`Match → TradeEvent`  
        `↓`  
`Clearing Engine`  
        `↓`  
`WalletEvents (debit / credit / unlock)`  
        `↓`  
`Wallet State Update`

This guarantees:

* Determinism

* Replayability

* Crash recovery

---

# **1️⃣1️⃣ Crash Recovery Guarantee**

On restart:

1. Replay WalletEvents

2. Rebuild wallet state

3. Rebuild positions

4. Resume matching

No inconsistencies.  
 No phantom money.

👉 **Invariant:**

If events are intact, balances are correct.

---

# **1️⃣2️⃣ Sequence Safety (Ties Everything Together)**

Every WalletEvent has:

`global_sequence_id`

Applied **in order**.

If:

* OMS retries

* Clearing replays

* Network splits

You still get the same wallet state.

---

# **1️⃣3️⃣ Final Mental Model**

Think of the wallet system as:

**A single-source-of-truth financial ledger that never thinks, never decides, only applies verified state transitions**

Everything else *asks permission*.

# **END-TO-END LIQUIDATION WALKTHROUGH**

We’ll assume:

* **USDT collateral**

* **Options on BTC**

* **Single collateral cross-margin**

* **Orderbook-based liquidation (auction → taker fallback)**

---

## **0️⃣ Pre-Liquidation Baseline**

### **User State (Before Trouble)**

`Wallet:`  
  `free_balance   = 2,000 USDT`  
  `locked_balance = 8,000 USDT`  
  `total          = 10,000 USDT`

`Positions:`  
  `Short 10 BTC 30,000 CALL (0.01 BTC lot → 1 BTC total)`  
  `Entry premium = 500 USDT`  
  `Initial margin = 8,000 USDT`  
  `Maintenance margin = 6,000 USDT`

---

## **1️⃣ Risk Engine Continuous Monitoring**

Risk engine subscribes to:

* Mark price feed

* Position state

* Wallet state

Every price tick:

`Equity = Wallet.total + Unrealized PnL`  
`Maintenance Requirement = Σ MM(position_i)`

---

### **Liquidation Trigger Condition**

`Equity <= Maintenance Margin`

Example:

`BTC price spikes`  
`Unrealized PnL = -4,500 USDT`

`Equity = 10,000 - 4,500 = 5,500`  
`MM = 6,000`

`⚠️ Breach detected`

👉 **Liquidation is now allowed, not mandatory yet**

---

## **2️⃣ Liquidation Eligibility Event**

Risk Engine emits:

`LiquidationEligible {`  
  `account_id,`  
  `equity,`  
  `maintenance_margin,`  
  `timestamp,`  
`}`

This does **not** mutate state.

👉 **Invariant:**  
 Risk engine never liquidates — it *flags*.

---

## **3️⃣ Liquidation Engine Takes Control**

Liquidation Engine:

* Consumes eligibility events

* Enforces **liquidation sequencing**

* Owns **position reduction logic**

---

## **4️⃣ Freeze the Account (CRITICAL)**

Before any action:

`OMS: trading_disabled = true`  
`Withdrawals: disabled`

No new orders.  
 No escape.

Wallet unchanged **for now**.

---

## **5️⃣ Determine Liquidation Quantity**

Liquidation engine computes:

`Required equity buffer = MM + liquidation_fee + slippage_buffer`

Example:

`Target equity = 6,500 USDT`  
`Current equity = 5,500`  
`Deficit = 1,000 USDT`

So it needs to:

* Close enough position to reduce risk

* Restore equity \> MM

---

## **6️⃣ Liquidation Strategy (Primary: Auction)**

### **Step 6.1 — Create Liquidation Order**

This is **not a user order**.

`Instrument: BTC 30,000 CALL`  
`Side: BUY (closing short)`  
`Quantity: 0.4 BTC worth`  
`Price: aggressive but bounded`  
`Order type: IOC`  
`Owner: LIQUIDATION_ENGINE`

👉 Goes **directly to Matching Engine**, bypassing OMS.

---

## **7️⃣ Matching Engine Executes Liquidation Trade**

Matching rules unchanged:

* Price-time priority

* Deterministic

* Atomic trades

Possible outcomes:

### **Case A: Full Fill**

`0.4 BTC closed`  
`Realized loss = -1,200 USDT`

### **Case B: Partial Fill**

`0.25 BTC filled`  
`Remaining stays open`

### **Case C: No Fill**

* Escalate to market taker

* Or trigger insurance

---

## **8️⃣ Trade → Clearing → Wallet Mutation**

Trade event emitted:

`TradeExecuted {`  
  `taker: LIQUIDATION_ENGINE,`  
  `maker: OTHER_TRADER,`  
  `price,`  
  `quantity,`  
`}`

Clearing engine processes:

### **8.1 Position Update**

`Short size reduced`

### **8.2 Realized PnL Applied**

`Wallet:`  
  `locked -= margin_released`  
  `locked -= realized_loss`

### **8.3 Liquidation Fee**

`Wallet:`  
  `locked -= liquidation_fee`  
`Insurance Fund:`  
  `credit += liquidation_fee`

👉 **All via WalletEvents**

---

## **9️⃣ Re-Evaluate Risk After Partial Liquidation**

Risk engine recalculates:

`New Equity`  
`New Maintenance Margin`

If:

`Equity > MM`

→ Stop liquidation.

Else:  
 → Repeat liquidation loop.

---

## **🔁 Liquidation Loop Summary**

`Detect breach`  
`→ Freeze account`  
`→ Place liquidation order`  
`→ Match`  
`→ Clear`  
`→ Update wallet`  
`→ Re-evaluate`  
`→ Repeat or stop`

---

## **🔥 10️⃣ Worst Case: Liquidation Fails**

### **Scenario**

* No liquidity

* Fast market

* Gap risk

Outcome:

`Equity < 0`

---

## **11️⃣ Insurance Fund Intervention**

`Deficit = abs(Equity)`

Clearing emits:

`InsuranceTransfer {`  
  `from: INSURANCE_FUND,`  
  `to: USER_WALLET,`  
  `amount: deficit`  
`}`

Wallet goes to zero.

User wiped out.  
 Exchange survives.

---

## **12️⃣ Absolute Worst Case: Insurance Exhausted**

Only then:

* Socialized loss

* Auto-deleveraging (ADL)

You can defer this.

---

## **13️⃣ Sequence Safety (Why This Never Breaks)**

Each step produces events:

`RiskEvent (read-only)`  
`LiquidationOrder`  
`TradeEvent`  
`ClearingEvent`  
`WalletEvent`

All with:

`global_sequence_id`

Replay → exact same liquidation.

---

## **14️⃣ What Makes This Design Correct**

### **Hard Invariants**

1. Wallet never goes negative without insurance backing

2. Liquidation orders bypass OMS but respect matching rules

3. Risk engine never mutates state

4. Clearing is the only writer

5. Everything is replayable

---

## **15️⃣ Mental Model You Should Lock In**

Liquidation is **forced trading under risk control**, not punishment.

The system:

* Reduces risk

* Preserves solvency

* Accepts loss gracefully

#  **EXACT LIQUIDATION PRICING ALGORITHM**

We’ll design this with **three goals**:

1. **Guaranteed execution when liquidity exists**

2. **No unnecessary loss to the user**

3. **Deterministic & replayable**

---

## **0️⃣ Inputs (What the Liquidation Engine Knows)**

At liquidation trigger time:

`Position:`  
  `instrument_id`  
  `side (LONG / SHORT)`  
  `size_remaining (contracts)`

`Market:`  
  `mark_price`  
  `best_bid`  
  `best_ask`  
  `order_book_snapshot (top N levels)`

`Risk:`  
  `maintenance_margin`  
  `equity`  
  `liquidation_fee_rate`

---

## **1️⃣ Compute Liquidation Target**

We **never close everything immediately** unless required.

### **Target Equity**

`target_equity = maintenance_margin + safety_buffer`

Example:

`MM = 6,000`  
`Safety buffer = 500`  
`Target = 6,500`

---

## **2️⃣ Compute Required Close Quantity**

For **short call** (risk ↑ when price ↑):

`PnL per contract ≈ option_delta × underlying_price_move`

But liquidation uses **worst-case conservative pricing**, not delta hedging.

So we estimate:

`loss_per_contract ≈ (execution_price - mark_price) × contract_multiplier`

Then:

`contracts_to_close =`  
  `ceil((target_equity - equity) / loss_per_contract)`

Bounded by:

`≤ position_size_remaining`

---

## **3️⃣ Liquidation Price Ladder (THIS IS THE CORE)**

We **do not** place a blind market order.

We use **progressive price bands**.

---

## **4️⃣ Price Band Construction**

### **Definitions**

`mid = (best_bid + best_ask) / 2`  
`spread = best_ask - best_bid`

### **Liquidation Side**

| Position | Liquidation Order |
| ----- | ----- |
| Long | SELL |
| Short | BUY |

---

### **Price Bands (BUY example – closing short)**

`Band 0: best_ask`  
`Band 1: best_ask + 0.25 * spread`  
`Band 2: best_ask + 0.5 * spread`  
`Band 3: best_ask + 1.0 * spread`  
`Band 4: mark_price * (1 + max_slippage)`

Where:

`max_slippage = 1–3% (configurable)`

---

## **5️⃣ Execution Loop (Deterministic)**

For each band:

`Place IOC order:`  
  `price = band_price`  
  `quantity = remaining_to_close`

Then:

`filled_qty = result.filled`  
`remaining -= filled_qty`

Stop if:

`remaining == 0`  
`OR equity >= target_equity`

---

## **6️⃣ Partial Fill Handling**

If IOC partially fills:

* Accept fill

* Recalculate:

  * equity

  * remaining position

  * required quantity

👉 **Never cancel filled trades**

---

## **7️⃣ When Price Bands Exhausted**

If after last band:

`remaining > 0`

Then:

### **Escalation Options (Configurable Order)**

1. **Market IOC**

2. **Trigger insurance fund**

3. **ADL (future)**

For v1:  
 → Market IOC → Insurance

---

## **8️⃣ Liquidation Fee Integration**

Liquidation fee is applied **per fill**, not upfront.

`fee = filled_notional × liquidation_fee_rate`

This:

* Incentivizes market makers

* Penalizes risky traders

* Feeds insurance fund

---

## **9️⃣ Determinism Guarantees**

All parameters are:

* Derived from order book snapshot

* Config-based

* Sequence-numbered

So replay yields identical price bands.

---

## **🔒 Critical Safety Constraints**

### **1\. Never Cross Instrument Boundaries**

Liquidation only trades the **same instrument**.

### **2\. Never Improve Price for User**

Liquidation orders are always **aggressive**, never passive.

### **3\. Never Place GTC Liquidation Orders**

All are IOC.

---

## **🧠 Why This Works**

* Uses **real liquidity**, not oracle fantasy

* Minimizes slippage

* Prevents cascading market nukes

* Deterministic

* Replay-safe

---

## **🧪 Concrete Example**

`Short 1 BTC CALL`  
`Mark = 2,000 USDT`  
`Best ask = 2,050`  
`Spread = 100`

`Bands:`  
`2050`  
`2075`  
`2100`  
`2150`  
`2160 (max slippage)`

Liquidation walks **up**, not jumps.

---

## **🔥 Common Failure Modes (That You Avoid)**

❌ Instant market order  
 ❌ Oracle-based liquidation  
 ❌ Liquidating full position blindly  
 ❌ Liquidation competing with user OMS orders  
 ❌ Non-deterministic retry logic

---

## **10️⃣ Minimal Pseudocode**

`for band_price in price_bands {`  
    `let fill = place_ioc(band_price, remaining);`  
    `apply_trade(fill);`  
    `remaining -= fill.qty;`

    `if equity() >= target_equity || remaining == 0 {`  
        `break;`  
    `}`  
`}`

`if remaining > 0 {`  
    `escalate();`  
`}`

