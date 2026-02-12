# **First: correct mental model**

**Clearing & Settlement is NOT one thing.**

It has **two distinct modes**:

1. **Continuous clearing (per trade)** → happens every match

2. **Terminal settlement (at expiry)** → happens once per instrument

Most confusion comes from mixing these.

---

# **1️⃣ What “Clearing & Settlement” actually means**

**Clearing** \= translating trades into obligations  
 **Settlement** \= fulfilling those obligations

In your architecture:

* Trades are immutable facts

* Positions, margins, PnL are **derived state**

* Nothing is ever “edited”, only appended

This aligns perfectly with your invariant.

---

# **2️⃣ Continuous clearing (after every trade)**

This happens **immediately after a trade is matched**.

### **Input event**

`TradeExecuted {`  
  `trade_id`  
  `instrument_id`  
  `buyer_id`  
  `seller_id`  
  `price`  
  `quantity`  
  `sequence`  
`}`

---

## **2.1 Position updates**

Positions are updated **symmetrically**.

### **Buyer**

* Increases LONG position

* Avg price updated

### **Seller**

* Increases SHORT position

* Avg price updated

No netting tricks. Just arithmetic.

---

### **Position update formula**

`new_qty = old_qty + trade_qty`  
`new_avg_price =`  
  `(old_qty × old_avg + trade_qty × trade_price) / new_qty`

For shorts, quantity is negative or tracked separately — your choice, but be consistent.

---

## **2.2 Margin transitions (this is subtle)**

There are **two kinds of margin** involved:

### **A. Reserved margin (from OMS approval)**

* Locked when order was accepted

* Assumed full fill

### **B. Actual margin (after trade)**

* Based on real position size

After trade:

* Reserved margin is partially released

* Actual margin is applied to position

This is how you stay solvent **during partial fills**.

---

## **2.3 Unrealized PnL (mark-based)**

PnL is **not settled yet**, only tracked.

`Unrealized PnL =`  
`(position_mark_price − avg_price) × qty × contract_size`

Important:

* Uses **mark price**, not last trade

* Prevents manipulation

---

## **2.4 Wallet balances (what changes now?)**

During continuous clearing:

* ❌ Wallet balance does NOT change

* ✅ Margin balance changes

* ✅ PnL changes (unrealized)

This separation is essential.

---

# **3️⃣ Funding vs Options (important clarification)**

You listed “Funding” — but:

👉 **Options do NOT have funding rates**  
 Funding is a **perpetual futures** concept.

So in your system:

* Funding \= ❌ not applicable (for now)

* Expiry settlement \= ✅ critical

Good instinct including it, but for options you can ignore funding entirely.

---

# **4️⃣ Expiry settlement (terminal settlement)**

This is where options *become real*.

Triggered by:

`instrument.status == EXPIRED`

Settlement happens **once**, deterministically.

---

## **4.1 Compute settlement price**

You must define **one authoritative settlement price**.

Standard choice:

`Settlement Price = Index Price at Expiry Timestamp`

Not last trade.  
 Not mark price.  
 Not oracle average (unless specified).

This must be:

* Deterministic

* Reproducible on replay

---

## **4.2 Payoff formulas (cash-settled)**

Let:

* `S_exp` \= settlement price

* `K` \= strike

* `Q × C` \= total exposure

---

### **Call option payoff**

`Payoff = max(S_exp − K, 0) × Q × C`

### **Put option payoff**

`Payoff = max(K − S_exp, 0) × Q × C`

---

## **4.3 Settlement transfers (zero-sum)**

For each matched contract:

| Party | Action |
| ----- | ----- |
| Long | Receives payoff |
| Short | Pays payoff |

This is **pure transfer** — no value creation.

---

## **4.4 Wallet updates (this is the only time balances move)**

At settlement:

* Margin is released

* PnL is realized

* Wallet balances are updated

* Positions are closed

After settlement:

`Position → 0`  
`Margin → 0`  
`PnL → realized`

The instrument is now **dead**.

---

# **5️⃣ Settlement sequencing (very important)**

Settlement is **not a batch job**.

Correct flow:

1. Instrument expires

2. Settlement event is sequenced

3. Settlement logic runs

4. Wallet updates emitted as events

5. Instrument moves to SETTLED

This guarantees:

* Replay safety

* No partial settlement

* No double execution

---

# **6️⃣ What if a user is insolvent at expiry?**

This is where margin discipline pays off.

If margin was correct:

* Short positions **always** have enough collateral

If not:

* Insurance fund absorbs loss

* Or auto-deleveraging (future feature)

But for v0:  
 👉 **Margin must guarantee settlement solvency**

No exceptions.

---

# **7️⃣ Clearing & settlement invariants (lock these)**

These are laws:

1. Trades are append-only

2. Positions are derived

3. Margin moves before wallet balance

4. Wallet balance moves only on:

   * Deposit

   * Withdrawal

   * Settlement

5. Settlement happens exactly once

6. Same events → same balances (always)

---

# **8️⃣ Minimal v0 clearing & settlement system**

You do **not** need complexity initially.

v0 requirements:

* Per-trade position updates

* Margin accounting

* Deterministic expiry settlement

* Cash-settled only

* No funding

* No early exercise

This is already institutional-grade if done correctly.

---

# **9️⃣ Mental compression (final)**

* **OMS** \= intent

* **Matcher** \= agreement

* **Clearing** \= obligation

* **Settlement** \= payment

Options only become *real money* at expiry.

