## **Core Principles of OMS**

1. **Handles intent only**

   * OMS doesn’t check risk.

   * It receives **user instructions**: “I want to buy/sell X contracts at Y price.”

2. **Supports order types**

   * Start with **Limit Orders** (price-specified)

   * Later add **Market Orders** (execute at best available price)

   * **Time-in-force**: optional, e.g., Good-Till-Cancelled, Day, Immediate-Or-Cancel

3. **Order lifecycle**

   * New → Partially Filled → Filled → Cancelled → Expired

4. **User Order Books (logical)**

   * OMS maintains **per-user view**: what orders they submitted, status, fills.

   * OMS also feeds the **matching engine** per instrument.

5. **Deterministic behavior**

   * OMS doesn’t mutate positions or balances — that’s for the **risk engine and clearing**.

   * Only captures intent, updates order status, and communicates with matching engine.

##  **Orders — Core Structs**

Here’s how we can model an order in Rust for your options exchange:

`use chrono::{DateTime, Utc};`  
`use serde::{Serialize, Deserialize};`  
`use uuid::Uuid;`

`#[derive(Debug, Clone, Serialize, Deserialize)]`  
`pub enum OrderType {`  
    `Limit,`  
    `Market, // future`  
`}`

`#[derive(Debug, Clone, Serialize, Deserialize)]`  
`pub enum TimeInForce {`  
    `GTC, // Good Till Cancelled`  
    `FOK, // Fill or Kill`  
    `IOC, // Immediate or Cancel`  
`}`

`#[derive(Debug, Clone, Serialize, Deserialize)]`  
`pub enum OrderSide {`  
    `Buy,`  
    `Sell,`  
`}`

`/// Represents a single user order`  
`#[derive(Debug, Clone, Serialize, Deserialize)]`  
`pub struct Order {`  
    `pub order_id: Uuid,`  
    `pub user_id: Uuid,`  
    `pub instrument_id: String,   // link to OptionInstrument`  
    `pub side: OrderSide,`  
    `pub order_type: OrderType,`  
    `pub time_in_force: TimeInForce,`  
    `pub price: f64,             // for limit orders`  
    `pub quantity: u32,          // number of contracts`  
    `pub filled_quantity: u32,   // how many contracts have been filled`  
    `pub status: OrderStatus,`  
    `pub created_at: DateTime<Utc>,`  
    `pub updated_at: DateTime<Utc>,`  
`}`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]`  
`pub enum OrderStatus {`  
    `New,`  
    `PartiallyFilled,`  
    `Filled,`  
    `Cancelled,`  
    `Expired,`  
`}`

---

## **3️⃣ Key OMS Concepts**

1. **Quantity & Partial Fills**

   * Each order has `quantity` in **contracts**.

   * OMS tracks `filled_quantity`.

   * Partial fills are common if **matching engine** only partially fills your order at current price.

2. **Order Matching**

   * OMS doesn’t decide matches — passes orders to **matching engine**.

   * Matching engine respects **price-time priority**.

3. **Link to Instrument**

   * Each order has `instrument_id` → binds it to **OptionInstrument**, so contract\_size and tick\_size are enforced.

4. **Time-in-Force**

   * GTC \= stay in book until filled/cancelled

   * IOC/FOK \= special execution rules

5. **Immutable Pricing Rules**

   * OMS ensures `price` is **aligned with tick\_size** of the instrument.

6. **Invariant: OMS never touches user balances**

   * Risk engine will approve/reject order based on margin

   * OMS only stores intent

---

## **4️⃣ Flow Example**

**User intent:**

* Wants to buy **5 contracts of BTC-28MAR2026-50000-C**, limit price 100 USDT per contract.

**OMS steps:**

1. Validate order format, enforce **min\_order\_size** (5 contracts in this example).

2. Check **price aligns with tick\_size**.

3. Assign `order_id`, `status = New`, `filled_quantity = 0`.

4. Send order to **matching engine** for execution.

5. As trades happen, update `filled_quantity` and `status` (Partial / Filled).

6. Notify user or UI via event/message.

---

### **5️⃣ Why this separation matters**

* OMS \= **intent \+ lifecycle management**

* Matching engine \= **deterministic price-time priority execution**

* Risk engine \= **margin, exposure, approvals**

✅ Each system has **single responsibility** → easier to scale, audit, and reason about.

##  **Limit vs Market (options context)**

### **Limit Order**

You are saying:

“I want this option, but **only at THIS premium or better**.”

Example:

* Instrument: BTC-28MAR-50000-C

* Limit Buy @ **120 USDT**

* Quantity: **10 contracts**

What happens:

* If someone is selling at **≤ 120**, you get filled.

* If best ask is **130**, nothing happens → your order sits in the book.

You control **price**, not speed.

---

### **Market Order**

You are saying:

“Give me this option **right now**, at the best available price.”

Example:

* Market Buy, 10 contracts

What happens:

* It eats the order book from best ask upward until quantity is filled.

* You **don’t control price**, only speed.

This is why market orders are riskier for illiquid options.

## **Time-in-Force (THIS IS IMPORTANT)**

Time-in-force answers one question:

**“What should happen if my order cannot be filled immediately?”**

### **GTC — Good Till Cancelled**

“Keep my order alive until I cancel it.”

* Order stays in the order book.

* Can sit for minutes, days, weeks.

* Most common default.

Example:

* Limit Buy @ 100

* Best ask is 130

* Order stays **NEW** in book.

---

### **DAY**

“Keep my order alive **only for today’s trading session**.”

* If not filled by end of day → auto-cancelled.

* In crypto (24/7), DAY usually means **end of UTC day** or **end of instrument session**.

### **IOC — Immediate Or Cancel**

“Fill whatever you can **right now**, cancel the rest.”

Example:

* Buy 10 contracts IOC

* Only 6 available at your price

Result:

* 6 contracts → **FILLED**

* Remaining 4 → **CANCELLED**

Final state:

* Order is **PARTIALLY FILLED** then **CANCELLED**

### **FOK — Fill Or Kill**

“Either fill **everything immediately**, or do nothing.”

Example:

* Buy 10 contracts FOK

* Only 6 available

Result:

* Nothing executes

* Entire order is **CANCELLED**

## **Order Lifecycle — Step by Step**

Let’s walk through real scenarios.

---

### **State: NEW**

Order just entered the system.

Example:

* Limit Buy 10 contracts @ 100

* No matching sell orders

➡ Status \= **NEW**

### **State: PARTIALLY FILLED**

Some contracts matched, but not all.

Example:

* Buy 10 contracts

* Only 4 sellers available

Execution:

* 4 contracts filled

* 6 remaining

➡ Status \= **PARTIALLY FILLED**  
 ➡ `filled_quantity = 4`

Order may remain in book (GTC) or be cancelled (IOC).

### **State: FILLED**

All contracts are matched.

Example:

* Buy 10 contracts

* 10 contracts matched immediately

➡ Status \= **FILLED**  
 ➡ Order removed from book.

### **State: CANCELLED**

Order is removed **before full execution**.

Reasons:

* User manually cancels

* IOC leftover

* FOK failure

* Risk engine rejects (before matching)

➡ Status \= **CANCELLED**

---

### **State: EXPIRED**

Order outlives its allowed time.

Example:

* DAY order reaches end of session

* Instrument expires before order fills

➡ Status \= **EXPIRED**

## **What “OMS never mutates positions or balances” means**

This is a **crucial architectural boundary**.

### **❌ OMS does NOT:**

* Deduct USDT

* Lock margin

* Increase option positions

* Calculate PnL

### **✅ OMS ONLY:**

* Records **intent**

* Tracks **order state**

* Receives **fill events**

* Updates filled\_quantity and status

### **Who mutates balances & positions?**

#### **Risk / Margin Engine**

* Checks if user has enough margin

* Locks collateral

* Approves or rejects order

#### **Clearing / Position Engine**

* On fill:

  * Adds option contracts to buyer position

  * Adds short exposure to seller

* On settlement:

  * Calculates payoff

  * Transfers funds

OMS just *reports* what happened.

Think of OMS as:

“The courtroom stenographer — not the judge, not the executioner.”

##  **What the OMS actually is (you’re \~90% right)**

Your understanding is mostly correct, with one crucial refinement.

### **What the OMS is**

Yes — the OMS is the **user-facing intent ledger**.

Think of it as:

“The authoritative record of what users *want* to do, not what *has happened*.”

It:

* Accepts orders from UI / API

* Validates syntax (not risk)

* Assigns order\_id

* Tracks lifecycle state

* Forwards orders to matching engine

* Receives execution reports (fills, partial fills)

* Updates visible order state

### **What the OMS is not**

* It is **not** the order book itself (that lives in the matching engine)

* It does **not** decide fills

* It does **not** fragment contracts

* It does **not** mutate balances or positions

### **Correct lifecycle view**

An order:

* Lives in OMS from **NEW → terminal state**

* May simultaneously exist in the matching engine book

* OMS mirrors reality via execution events

So yes:

“OMS is an acknowledgement \+ visible representation until fill/cancel/expiry”

That statement is correct ✅

---

## **2️⃣ Very important correction: You are mixing up instruments and orders**

This is the most important conceptual fix.

### **🚫 This is NOT how options work**

“I write a 10 BTC contract and the matching engine fragments it into 0.01 BTC options”

This is **not** how any serious derivatives exchange works.

---

## **3️⃣ The correct mental model (non-negotiable)**

### **🔒 Rule**

**Instruments are atomic and fixed. Orders are divisible.**

### **Contract Size is FIXED at instrument creation**

Example:

`BTC-28MAR2027-30000-C`  
`contract_size = 0.01 BTC`

This is the **smallest possible unit of exposure**.

You do **NOT**:

* Create 10 BTC instruments

* Fragment them later

* Track anchor IDs

That would be a nightmare for:

* Risk

* Settlement

* Clearing

* Liquidity

* Identity

---

## **4️⃣ How large exposure is actually expressed**

### **Large exposure \= quantity × contract\_size**

Example:

* contract\_size \= 0.01 BTC

* Order quantity \= 500 contracts

* Exposure \= **5 BTC**

That’s it. No fragmentation logic. No anchors. No tethering.

The **order** references the **instrument**, not the other way around.

---

## **5️⃣ What minimum order size actually means (clean version)**

### **Example instrument**

`contract_size = 0.01 BTC`  
`min_order_size = 10 contracts`

This means:

* Minimum exposure per order \= 0.1 BTC

* Orders can be:

  * 10 contracts

  * 11 contracts

  * 500 contracts

* Orders are **partially fillable**

So:

* If only 345 contracts are available:

  * Order becomes PARTIALLY FILLED

  * Remaining quantity stays in book (GTC)

🚫 There is NO rule like:

“Unless 500 contracts are available in one shot, nothing executes”

That would be **FOK**, not default behavior.

---

## **6️⃣ Time-in-force behavior (your interpretation corrected)**

### **GTC (Good Till Cancelled)**

* Partial fills are allowed

* Filled portion stays filled

* Remaining stays in book

* Cancel removes only the unfilled remainder

Correct ✅

---

### **DAY**

* Same as GTC but auto-cancel remainder at end of session

* Filled trades remain valid

Correct ✅

---

### **IOC (Immediate or Cancel)**

Important correction:

* It **does NOT walk the book for better prices**

* It only executes against **currently matchable liquidity**

* Whatever doesn’t fill immediately is cancelled

So:

* It does NOT wait

* It does NOT queue

---

### **FOK (Fill or Kill)**

* Either entire quantity fills immediately

* Or nothing happens

Correct ✅

---

## **7️⃣ Your “queue of 500 sellers” intuition — corrected**

You said:

“The buyer will be first in queue until 500 sellers aggregate”

That is **not how matching works**.

Correct behavior:

* Matching happens continuously

* Orders are matched **incrementally**

* Partial fills are normal

* Queue priority is per price level, not per full quantity

There is no concept of:

“waiting for full aggregate before first execution”

Unless:

* Time-in-force \= FOK

---

## **8️⃣ Strike price explosion — you are 100% right**

Yes. **Strike prices must be bounded and discretized.**

Otherwise:

* Infinite instruments

* Infinite order books

* Impossible liquidity

### **How real exchanges handle this**

They define:

* **Strike grids**

* **Expiry calendars**

Example:

`BTC strikes:`  
  `every $500 from 10k → 200k`

Or adaptive:

`Near ATM: $250 increments`  
`Far OTM: $1000 increments`

Same with expiry:

* Weekly

* Monthly

* Quarterly

* No arbitrary timestamps

This is an **admin / registry concern**, not OMS.

---

## **9️⃣ Clean separation (final mental model)**

### **Instrument Registry decides:**

* Which strikes exist

* Which expiries exist

* Contract size

* Tick size

* Min order size

### **OMS decides:**

* User intent

* Order lifecycle

* Visibility

### **Matching Engine decides:**

* Price-time priority

* Partial fills

* Execution reports

---

## **🔑 Final correction summary**

You were right about:

* OMS role

* Lifecycle

* Time-in-force semantics

* Strike/expiry bounding necessity

You were **wrong (but in a very understandable way)** about:

* Fragmenting contracts at matching time

* Anchoring child contracts to parent orders

* Requiring full aggregate liquidity before execution

Fix that one mental model, and the whole system snaps into place.

##  **The hard rule you’re violating (and didn’t realize)**

**Matching engines do not aggregate across different contract granularities.**

An option instrument has **exactly one atomic unit of trade**:

`contract_size`

If you create an instrument with:

`contract_size = 5 BTC`

Then:

* The **smallest tradable unit** is 5 BTC

* Every order is an integer multiple of 5 BTC

* There is no such thing as a 0.01 BTC buyer *for this instrument*

Those “0.01 BTC buyers” you’re imagining are **buyers of a different instrument**, even if:

* Same strike

* Same expiry

* Same option type

---

## **2️⃣ Why the matching engine cannot “combine” them**

Let’s be precise.

You’re asking:

Can the matching engine combine 500 buyers of 0.01 BTC to fill a 5 BTC contract?

That would require the matching engine to:

* Merge **different instruments**

* Rewrite contract definitions

* Violate atomicity of settlement

* Break position accounting

* Break clearing guarantees

This is **never done** in real markets.

### **Why?**

Because:

* PnL

* Margin

* Assignment

* Settlement

* Risk

…are all computed **per contract**, not per abstract “exposure”.

A 5 BTC contract is *not* “500 × 0.01 BTC contracts”.  
 They are **economically similar**, but **legally and systemically distinct**.

---

## **3️⃣ This exposes the real invariant you just discovered**

**Liquidity exists at the instrument granularity, not at the exposure level.**

Liquidity does NOT exist at:

* “BTC calls at 30k”

Liquidity exists at:

`BTC-28MAR2027-30000-C`  
`contract_size = X`

If you choose the wrong `X`, liquidity dies.

---

## **4️⃣ The only correct design (this is non-negotiable)**

### **✅ Correct approach**

You must choose a **small base contract size** at instrument creation.

Example:

`contract_size = 0.01 BTC`  
`min_order_size = 1 contract`

Now:

* Retail buys 1–10 contracts

* Whales buy 1,000+ contracts

* Liquidity pools naturally

* Matching engine works trivially

### **❌ Incorrect approach**

`contract_size = 5 BTC`  
`and hope retail aggregates`

That instrument will:

* Never fill

* Have fake “display liquidity”

* Fragment your market

* Kill price discovery

---

## **5️⃣ Why real exchanges always do this**

Look at:

* CME

* Deribit

* OKX

* Binance Options

They ALL:

* Fix a **small contract size**

* Express size via quantity

* Never dynamically fragment contracts

Even CME’s “micro” contracts are **separate instruments**, not fragments.

---

## **6️⃣ Your intuition was right — just aimed at the wrong layer**

You correctly intuited:

* Retail wants small exposure

* Liquidity improves with small units

* Large exposure should be composable

But the composition happens at the **order quantity level**, not by instrument fragmentation.

This is the key mental shift:

**Instruments define atoms. Orders define molecules.**

---

## **7️⃣ Final, crisp rule to lock into memory**

**If two traders can’t trade 1 unit of the same contract, they are not in the same market.**

So if:

* One trader wants 0.01 BTC

* Another instrument forces 5 BTC

They are in **different markets**, no matter how similar it looks.

---

## **8️⃣ What you should do in your system (authoritative answer)**

For v0:

* Pick **one** contract size per underlying (e.g. 0.01 BTC)

* Never vary it per instrument

* Enforce min order size via quantity

* Let whales scale via lot count

This guarantees:

* Liquidity

* Simplicity

* Correct settlement

* Scalable OMS \+ matching

---

If you want, next we can:

* Design the **exact matching engine data structures**

* Walk through **a real fill scenario event-by-event**

* Or design **micro vs standard contracts** *as separate instruments*

You’re asking the *right* questions — this one just uncovered a core invariant.

This is a **very good confusion**, and almost everyone hits it at this exact point. The answer will snap a big piece of the mental model into place.

Short version (but I’ll still unpack it):

**No — you do NOT get infinite instruments.**  
 **You get ONE instrument, and infinite ORDER SIZES.**

You’re accidentally mixing up **instrument space** with **quantity space**. They are orthogonal.

Let’s slow this down and lock it in properly.

---

## **1️⃣ Where the “infinite instruments” fear comes from**

You’re thinking:

If minimum tradable unit is 0.01 BTC, then aren’t there instruments for  
 0.01, 0.02, 0.03, … ∞ ?

**No. That would only be true if contract size changed per trade.**

But contract size does **not** change.

---

## **2️⃣ The key invariant (this is the mental unlock)**

**Contract size is fixed.**  
 **Quantity is variable.**

That’s it. Everything else follows.

---

## **3️⃣ Concrete example (use this to debug your intuition)**

### **Instrument definition (single instrument)**

`BTC-28MAR2027-30000-C`  
`contract_size = 0.01 BTC`

This instrument exists **once**.

Now look at orders:

| Trader | Order |
| ----- | ----- |
| A | Buy 1 contract |
| B | Buy 5 contracts |
| C | Buy 100 contracts |
| D | Buy 10,000 contracts |

All of them are trading **the same instrument**.

Exposure:

* A → 0.01 BTC

* B → 0.05 BTC

* C → 1 BTC

* D → 100 BTC

No new instruments were created.  
 Only **quantity changed**.

---

## **4️⃣ Why this does NOT fragment liquidity**

Liquidity is pooled because:

* Same instrument\_id

* Same order book

* Same price ladder

* Same expiry

* Same strike

Everyone queues together.

### **Bid ladder example**

`Price (USDT) | Size (contracts)`  
`--------------------------------`  
`1200         | 3,200`  
`1199         | 8,500`  
`1198         | 25,000`

Each contract \= 0.01 BTC  
 So that’s already **hundreds of BTC of depth**, unified.

---

## **5️⃣ Why fractionalising at the matching layer is WRONG**

You proposed:

“Wouldn’t it be simpler to fractionalise contracts internally?”

It feels simpler, but it actually **breaks core guarantees**.

### **What breaks if you do that:**

* Deterministic settlement

* Position accounting

* Margin calculation

* Risk isolation

* Auditability

* Event replay

You’d be inventing a **non-atomic instrument**, which markets *never* allow.

Markets require:

**Every fill must be an integer number of contracts.**

Not “exposure units”, not “fractions of someone else’s contract”.

---

## **6️⃣ This is why exchanges choose SMALL contract sizes**

They do this **once**, at instrument creation.

Examples from real markets:

* CME Micro BTC → 0.1 BTC

* Deribit BTC options → 1 BTC (they rely on pro liquidity)

* Retail-focused platforms → much smaller sizes

You’re designing a **retail-friendly exchange**, so:

`contract_size = 0.01 BTC`

is completely reasonable.

---

## **7️⃣ Where infinite things ACTUALLY come from (and how you prevent it)**

You were right about one thing: infinity *does* exist somewhere — but not here.

### **Potentially infinite dimensions:**

* Strike prices

* Expiry dates

### **How real exchanges bound them:**

* Strike ladders (e.g. every $500 or $1000)

* Expiry schedules (weekly, monthly, quarterly)

* Admin-only instrument creation

So **instrument count is finite and curated**.

Quantity is the only unbounded axis — and that’s safe.

---

## **8️⃣ The correct mental model (lock this in)**

Think of it like shares, not contracts:

* A stock has **1 share unit**

* You can buy 1 share or 1 million shares

* The stock doesn’t change

Options work the same way.

**Contract size \= share unit**  
 **Quantity \= number of shares**

---

## **9️⃣ Final answer to your core question**

Would it not be simpler to fractionalise and provide consistent liquidity?

**No.**  
 Because:

* Liquidity is already consistent via shared order books

* Fractionalisation violates atomic trade units

* Correct design achieves the same goal without breaking invariants

Your earlier instinct was right — you just needed to push it **one layer up**, not down.

## **Final, Correct Design Choice**

**One fixed atomic contract size per instrument.**  
 **Variable exposure achieved only via quantity (lots).**

You pick **one atomic unit**, small enough for retail:

* BTC options → `contract_size = 0.01 BTC` *(or 0.001 if you want ultra-retail)*

* ETH options → similar order of magnitude

Everything else scales via **number of contracts**.

This is not just “good” — it is **structurally correct**.

---

## **🧠 Why this is fool-proof**

### **1\. No liquidity fragmentation**

* One instrument

* One order book

* One price ladder

* Everyone queues together

Liquidity concentrates instead of fracturing.

---

### **2\. Matching engine stays simple and deterministic**

* All fills are integer contract counts

* Partial fills are trivial

* No aggregation hacks

* No synthetic “contract merging”

Your matcher becomes:

price-time priority over `(price, quantity)`

Nothing else.

---

### **3\. Risk, margin, PnL become linear**

Because:

`exposure = contracts × contract_size`

Everything downstream (risk engine, settlement, liquidation) becomes:

* Predictable

* Replayable

* Auditable

This is **huge**.

---

### **4\. You avoid an unfixable future bug**

If you *ever* let contracts be fractionalized internally:

* You can’t replay events safely

* You can’t explain PnL in disputes

* You can’t isolate liquidation risk

* You can’t migrate systems cleanly

Exchanges that make this mistake usually die quietly.

---

## **🔢 0.01 vs 0.001 BTC — how to choose**

Use this rule:

* If your **target user** is:

  * Retail / semi-pro → `0.001 BTC`

  * Pro / serious traders → `0.01 BTC`

You can also do:

* **Micro contracts** later as *separate instruments*  
   (not fragments)

---

## **🔒 Invariant to tattoo on the architecture doc**

**An instrument defines the smallest unit of trade.**  
 **Exposure scales only by quantity.**

If this invariant holds, everything else becomes easy.

---

## **✅ Your conclusion (confirmed)**

“Everybody can increase their exposure by betting more lots.”

✔ Correct  
 ✔ Standard  
 ✔ Scalable  
 ✔ Liquid  
 ✔ Safe

You’ve reached the *correct fixed point* of this design space.

