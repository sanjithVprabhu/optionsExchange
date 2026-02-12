## **What the Matching Engine IS (and what it is NOT)**

**It is:**

* A **pure function over orders**

* A **deterministic state machine**

* A **price-time priority enforcer**

* A **trade generator**

**It is NOT:**

* ❌ A balance checker

* ❌ A margin calculator

* ❌ A risk engine

* ❌ A settlement system

Think of it as:

“Given an order book and a new order, produce zero or more trades and a new order book — deterministically.”

That’s it.

## **Core Invariant (This is sacred)**

**Given the same initial book \+ same order stream → same trades, always**

This single invariant drives **every architectural decision**.

If you ever violate this:

* You can’t replay

* You can’t audit

* You can’t simulate

* You can’t scale safely

##  **Per-Instrument Order Books**

There is **no global book**.

You have:

`BTC-USD book`  
`ETH-USD book`  
`SOL-USD book`

Each book is **completely independent**.

### **Structure of one order book**

Conceptually:

`OrderBook {`  
  `bids: PriceLevels (descending)`  
  `asks: PriceLevels (ascending)`  
`}`

Each **price level**:

`PriceLevel {`  
  `price`  
  `queue: FIFO list of orders`  
`}`

### **Why FIFO?**

Because of **time priority**.

Once price is fixed, **earliest order always wins**.

## **Price–Time Priority (Formally Defined)**

Matching priority is a **total ordering**:

1. **Better price**

2. **Earlier time**

No exceptions.  
 No randomness.  
 No batching tricks.

This ordering must be:

* Stable

* Deterministic

* Replayable

## **Per-Instrument Order Books**

There is **no global book**.

You have:

`BTC-USD book`  
`ETH-USD book`  
`SOL-USD book`

Each book is **completely independent**.

### **Structure of one order book**

Conceptually:

`OrderBook {`  
  `bids: PriceLevels (descending)`  
  `asks: PriceLevels (ascending)`  
`}`

Each **price level**:

`PriceLevel {`  
  `price`  
  `queue: FIFO list of orders`  
`}`

### **Why FIFO?**

Because of **time priority**.

Once price is fixed, **earliest order always wins**.

---

## **2️⃣ Price–Time Priority (Formally Defined)**

Matching priority is a **total ordering**:

1. **Better price**

2. **Earlier time**

No exceptions.  
 No randomness.  
 No batching tricks.

This ordering must be:

* Stable

* Deterministic

* Replayable

---

## **3️⃣ Order Types the Matching Engine Cares About**

The matching engine only understands **intent**, not balances.

Minimal set:

* `Limit Buy`

* `Limit Sell`

* `Market Buy`

* `Market Sell`

* (Optionally IOC / FOK later)

Each order has:

`order_id`  
`side (buy/sell)`  
`price (optional for market)`  
`quantity_remaining`  
`timestamp / sequence_number`

⚠️ **Sequence number \> timestamp**  
 You already know this, but it matters deeply here.

---

## **4️⃣ Deterministic Matching Loop (This Is the Core Algorithm)**

Let’s say a **new BUY order** arrives.

### **Step-by-step logic**

#### **Step 1: Select opposing book**

* Buy order matches against **asks**

* Sell order matches against **bids**

#### **Step 2: Check price crossing**

* Buy crosses if: `best_ask.price ≤ buy.price`

* Sell crosses if: `best_bid.price ≥ sell.price`

If **no crossing**:

* If limit → insert into book

* If market → cancel remainder

#### **Step 3: Match at best price level**

At a given price level:

* Iterate orders **FIFO**

* Consume quantities

For each match:

`trade_qty = min(incoming.qty, resting.qty)`

#### **Step 4: Emit an atomic trade**

`Trade {`  
  `taker_order_id`  
  `maker_order_id`  
  `price (maker price)`  
  `quantity`  
  `sequence_number`  
`}`

⚠️ Price is **always maker price**, never taker price.

---

## **5️⃣ Atomic Trades (Extremely Important)**

Each match produces **one atomic trade**.

Atomic means:

* Either fully recorded

* Or not recorded at all

No partial side effects.

Why this matters:

* Crash safety

* Replay correctness

* Downstream settlement

### **Example**

Incoming BUY 10 @ 100  
 Book has:

* Sell 4 @ 99

* Sell 7 @ 100

Produces **two trades**:

`Trade 1: 4 @ 99`  
`Trade 2: 6 @ 100`

Remaining 1 → goes into book (limit) or canceled (market)

---

## **6️⃣ No Balance Logic (By Design)**

The matching engine:

* Does **not** know user balances

* Does **not** know margin

* Does **not** reject for insufficient funds

Why?  
 Because **determinism dies** if you mix external state.

Balances live **outside**, usually enforced at:

* Order acceptance

* Risk checks

* Pre-trade validation

Matching engine assumes:

“If the order reached me, it is valid.”

---

## **7️⃣ Determinism: How It’s Actually Enforced**

This is subtle and critical.

### **Determinism requires:**

1. **Single-threaded matching per book**

   * Or strictly ordered event queue

2. **Monotonic sequence numbers**

   * Assigned *before* matching

3. **No wall-clock time**

   * Never use `now()` inside matching

4. **Pure data structures**

   * No random iteration order

   * No hash maps without ordering guarantees

If you replay:

`same order stream`  
`same sequence numbers`  
`same initial state`

You **must** get:

`byte-for-byte identical trades`

---

## **8️⃣ Insert vs Match vs Cancel (State Transitions)**

Every incoming order results in **exactly one** of:

1. Fully matched → order gone

2. Partially matched → remainder inserted or canceled

3. Not matched → inserted

4. Market remainder → canceled

There is **no fifth outcome**.

This is important for correctness proofs.

---

## **9️⃣ Matching Engine as a State Machine**

You can model the engine as:

`State:`  
  `OrderBook`

`Input:`  
  `OrderEvent`

`Output:`  
  `[TradeEvents] + BookDelta`

No side effects.  
 No callbacks.  
 No external calls.

This is why engines like Nasdaq, CME, and crypto exchanges can:

* Replay entire days

* Rebuild books from scratch

* Prove correctness under audit

---

## **10️⃣ What We Have NOT Covered Yet (So You Know What’s Next)**

We’ve understood the **core matching logic**.  
 Still left:

* Order cancel semantics

* Amend/replace orders

* Iceberg orders (later)

* Self-trade prevention

* Auction / opening cross

* Batch vs continuous matching

* Lock-free vs single-threaded designs

* Persistence & recovery

But before *any* of that…

---

## **Next Logical Step**

If you agree we’ve nailed the **conceptual engine**, the next step is:

### **👉 Design the exact data structures**

* Price ladder representation

* FIFO queues

* Sequence handling

* Memory layout (this matters for speed)

## **Instrument isolation: absolutely strict**

**The matching engine ONLY matches within the same instrument**

Your examples are spot-on:

* ✅ `BTC-USDT | 30000 CALL | Feb 13 2026`

* ❌ **never mixed with** `BTC-USDT | 30000 CALL | Feb 14 2026`

* ❌ **never mixed with** `BTC-USDT | 33000 CALL | Feb 13 2026`

Each of these is a **different instrument\_id**.

So conceptually:

`instrument_id = {`  
  `underlying: BTC-USDT`  
  `type: CALL`  
  `strike: 30000`  
  `expiry: 2026-02-13`  
`}`

👉 **One instrument \= one order book \= one matching loop**

There is zero cross-pollination.

---

## **2️⃣ Premium movement happens *inside* that single book**

For  
 `BTC-USDT | 30000 CALL | Feb 13 2026`

You’ll have:

* Multiple buyers bidding different **premiums**

* Multiple sellers asking different **premiums**

Premium goes up or down **only because**:

* Higher bids arrive

* Lower asks arrive

* Trades occur at the best prices

Not because of anything external.

---

## **3️⃣ Yes: all bids & asks live inside ONE order book**

For that instrument, the structure is exactly this:

`OrderBook {`  
  `bids: PriceLevels (descending)`  
  `asks: PriceLevels (ascending)`  
`}`

Example (premiums):

`Asks:`  
  `520 → [orderA, orderB]`  
  `510 → [orderC]`

`Bids:`  
  `500 → [orderD, orderE]`  
  `490 → [orderF]`

Each **PriceLevel** contains:

* The price (premium)

* A FIFO queue of orders at that price

---

## **4️⃣ FIFO inside price level — non-negotiable**

Within a price:

* First order in \= first order matched

* No jumping the queue

* No reordering

This guarantees **time priority**.

---

## **5️⃣ New order flow (this is exactly correct)**

Let’s walk your described flow and confirm it line-by-line.

### **A new order arrives (say a BUY limit)**

1. Engine selects **opposing side** (asks)

2. Checks **best ask**

If:

 `best_ask.price ≤ buy.price`

3.  → crossing exists

4. Engine:

   * Starts at **best ask price**

   * Iterates FIFO orders

   * Matches quantities

   * Emits atomic trades

   * Moves to next ask price if needed

5. Stops when:

   * Incoming quantity \= 0, or

   * No more price overlap

6. If remainder exists:

   * Limit → inserted into bids at its price

   * Market → remainder canceled

✅ That description you gave is **100% accurate**.

---

## **6️⃣ Important nuance: price ladder ≠ one price**

When we say “ask/bid ladder”, we mean:

* Many price levels

* Strict ordering

* Only **best prices are touched first**

The engine **never jumps levels**.

---

## **7️⃣ Mental model that will never fail you**

Think of the matching engine as:

“A per-instrument conveyor belt that consumes orders in sequence order, walks a sorted price ladder, and emits trades deterministically.”

No balances.  
 No expiry logic.  
 No Greeks.  
 No PnL.

Just matching.

## **What a price ladder MUST do (non-negotiables)**

For **each instrument**, the matching engine needs:

### **Core operations**

* Insert order at `(price, time)`

* Remove order (cancel / fully filled)

* Peek best bid / best ask

* Iterate price levels in strict order

* FIFO inside each price level

### **Performance expectations**

* O(log N) for price discovery

* O(1) FIFO operations per level

* Deterministic iteration order

* Stable under replay (same inputs → same matches)

👉 This immediately rules out naïve hash maps.

---

## **2️⃣ Canonical data structure choices**

### **Option A — TreeMap / BTreeMap (most common)**

`BTreeMap<Price, PriceLevel>`

* Bids: ordered **descending**

* Asks: ordered **ascending**

Each `PriceLevel`:

`PriceLevel {`  
  `price`  
  `queue: VecDeque<Order>`  
`}`

#### **Pros**

* Deterministic

* Simple

* Excellent for correctness

* Easy to replay

* Used by many real exchanges

#### **Cons**

* Slightly slower than arrays

* But totally fine unless you’re chasing microseconds

---

### **Option B — Skip List**

Used in ultra-low-latency systems.

Pros:

* Log time

* Concurrent-friendly

Cons:

* Harder to reason about

* Easy to introduce nondeterminism

* Overkill for v1

❌ I would not start here.

---

### **Option C — Fixed price grid (array indexed)**

Example:

`price_index = (price - min_price) / tick_size`

Used by:

* CME-style futures

* Highly constrained markets

Pros:

* O(1) lookup

* Extremely fast

Cons:

* Requires bounded price space

* Needs pre-defined min/max

* Not flexible for crypto options premiums

⚠️ Maybe later. Not now.

---

## **3️⃣ The structure I recommend for YOU**

You want:

* Determinism

* Replayability

* Clean reasoning

* Easy auditability

### **Use this:**

`OrderBook {`  
  `bids: BTreeMap<Price, PriceLevel>   // descending`  
  `asks: BTreeMap<Price, PriceLevel>   // ascending`  
`}`

In Rust terms:

`bids: BTreeMap<Reverse<Price>, PriceLevel>`  
`asks: BTreeMap<Price, PriceLevel>`

---

## **4️⃣ What is a PriceLevel, exactly?**

`PriceLevel {`  
  `price: Price`  
  `orders: VecDeque<OrderId>`  
`}`

Why `OrderId` and not full order?

* Orders live in a central order store

* Price level just references them

* Cancellation is easier

* Memory locality is better

Central store:

`orders: HashMap<OrderId, Order>`

---

## **5️⃣ Order object (minimal)**

`Order {`  
  `order_id`  
  `side        // BUY or SELL`  
  `price`  
  `quantity_remaining`  
  `timestamp   // sequence number, not wall clock`  
`}`

⚠️ **Timestamp MUST be engine sequence-based**, not real time.

---

## **6️⃣ How FIFO is guaranteed**

FIFO is guaranteed by:

* Append to `VecDeque` on insert

* Pop from front on match

Never reorder.  
 Never splice.

If an order is partially filled:

* It stays at the front

* Quantity decreases

* It does NOT move

This is critical.

---

## **7️⃣ Matching loop (price ladder traversal)**

Pseudo-logic (buy order):

`while buy.qty > 0:`  
  `best_ask = asks.first_key()`  
  `if best_ask > buy.price:`  
    `break`

  `level = asks[best_ask]`  
  `while level not empty and buy.qty > 0:`  
    `maker = level.front()`  
    `trade_qty = min(buy.qty, maker.qty)`  
    `emit_trade()`  
    `decrement both`

    `if maker.qty == 0:`  
      `pop_front(level)`

  `if level empty:`  
    `remove price level`

Mirror logic for sell.

This is **pure**, deterministic, and atomic.

---

## **8️⃣ Determinism guarantee (this matters a LOT)**

Matching is deterministic because:

* Price ordering is stable

* FIFO inside price

* Single-threaded per instrument (or strict sequencing)

* No randomness

* No clocks

* No balances involved

Same order stream → same trades → same history.

This is foundational for:

* Audits

* Replay

* Disaster recovery

* Dispute resolution

---

## **9️⃣ Subtle edge cases you WILL hit**

### **1\. Cancel while partially filled**

* Remove from middle of VecDeque

* Solution: doubly-linked list or index map

* OR mark canceled \+ lazy cleanup

Most exchanges use **lazy removal**.

---

### **2\. Empty price levels**

Always delete empty levels immediately.

---

### **3\. Market orders**

* Treated as limit with ∞ (buy) or 0 (sell)

* Must never rest in book

---

### **4\. Tick size enforcement**

Price must be validated **before** reaching matching engine.

---

## **10️⃣ You now fully understand the price ladder**

At this point:

* You could implement a working matching engine

* You are thinking like an exchange engineer, not a trader

* The remaining complexity is **sequencing & concurrency**, not logic

## **What determinism actually means**

**Determinism \= same inputs → same outputs**

In your case:

`Same order events (in same order)`  
`→ same trades`  
`→ same fills`  
`→ same order book state`

Why this matters:

* Regulatory audits

* Dispute resolution

* Crash recovery

* Backtesting against production logic

* Distributed replication

If two nodes replay the same log and get different books → your exchange is broken.

---

## **2️⃣ Why wall-clock time is poison ☠️**

Wall-clock timestamps:

* Drift

* Jump backward (NTP)

* Differ across machines

* Change under load

**Never use real time for ordering decisions.**

You may *store* wall time for UX and reports, but it must **never** affect matching.

---

## **3️⃣ Sequence numbers: the real clock**

### **Single source of truth**

You introduce a **monotonic sequence number**:

`u64 seq_no`

Properties:

* Starts at 1

* Increments by 1 per event

* Never skips

* Never repeats

* Never resets

Every meaningful event gets one:

* Order accepted

* Order canceled

* Trade executed

* Order expired

---

### **Order timestamp ≠ wall time**

Instead of:

`created_at = Instant::now()`

You do:

`created_seq = seq_no`

This is what enforces **price-time priority**.

---

## **4️⃣ Where sequence numbers live**

### **Option A — Global sequence (recommended)**

One sequence generator for the entire exchange.

Pros:

* Simple replay

* Easy auditing

* Total ordering across instruments

Cons:

* Slightly more contention (manageable)

This is what **most exchanges do**.

---

### **Option B — Per-instrument sequence**

Each instrument has its own counter.

Pros:

* Parallelism

* Lower contention

Cons:

* Harder replay

* Cross-instrument causality is ambiguous

* More complex recovery

⚠️ Only do this if you really need scale.

👉 **Start with global.**

---

## **5️⃣ The event log (this is the spine)**

Your exchange is really an **event processor**.

### **Canonical event structure**

`Event {`  
  `seq_no`  
  `event_type`  
  `payload`  
`}`

Examples:

`OrderAccepted`  
`OrderCanceled`  
`TradeExecuted`  
`OrderExpired`

The matching engine:

* Consumes events

* Emits events

* Never mutates state silently

---

### **Append-only log**

Rules:

* Events are immutable

* Written before state mutation (WAL)

* Totally ordered by seq\_no

This is what allows:

* Replay

* Recovery

* Replication

Think Kafka-style, but deterministic.

---

## **6️⃣ How determinism is enforced end-to-end**

### **Flow**

1. OMS validates intent

2. Sequencer assigns `seq_no`

3. Event appended to log

4. Matching engine consumes event

5. Matching engine emits trade events

6. Trade events appended with new seq\_nos

7. State mutates strictly in seq order

No shortcuts.

---

## **7️⃣ Crash recovery (non-negotiable)**

On restart:

`state = empty`  
`for event in log ordered by seq_no:`  
  `apply(event)`

If your engine is deterministic:

* You end up in **exact same state**

* Order books match bit-for-bit

* Positions reconcile perfectly

If not → you find bugs.

---

## **8️⃣ Concurrency WITHOUT nondeterminism**

This is where many systems fail.

### **Rule**

**Only one thread may mutate an instrument’s order book at a time.**

Ways to do this safely:

#### **Option 1 — Single threaded engine (simplest)**

* One event loop

* Deterministic

* Easy correctness

Perfect for v1.

---

#### **Option 2 — Sharded by instrument**

* Each instrument has its own queue

* Events routed by instrument\_id

* Global seq\_no still assigned first

Still deterministic if:

* Each instrument processes events in seq order

* No shared mutable state

---

## **9️⃣ Matching engine is a pure function**

Conceptually:

`(new_state, emitted_events) = f(old_state, input_event)`

No:

* Time

* Randomness

* Network

* Balances

* Side effects

This purity is why replay works.

---

## **🔥 Key invariants (tattoo these)**

* Sequence number \> time

* Event log \> in-memory state

* Determinism \> parallelism

* Replayability \> micro-latency

# **Options-specific nuances (outside matching engine)**

This is a critical mental correction:

**The matching engine does not know it is matching options.**

It matches **units**.

Everything “options-specific” is *metadata interpreted elsewhere*.

---

## **Atomic trading unit (this settles your lot confusion)**

You already arrived at the correct conclusion intuitively:

**One atomic contract size** (e.g. 0.01 BTC)  
 Exposure \= number\_of\_contracts × contract\_multiplier

So:

`contract_multiplier = 0.01 BTC`  
`order.quantity = 137 contracts`  
`exposure = 1.37 BTC`

### **Why this is non-negotiable**

* No fragmentation logic in matching

* No special-case fills

* No liquidity splitting

* Infinite scalability

* Identical logic for futures, options, perps

👉 **Matching engine only sees integers.**

---

## **Where option semantics live**

| Concern | Lives in |
| ----- | ----- |
| Contract multiplier | Instrument metadata |
| Strike price | Instrument metadata |
| Expiry | Instrument lifecycle |
| Call / Put | Instrument metadata |
| European exercise | Settlement engine |
| Payoff | Settlement engine |
| PnL | Risk / margin engine |

**Not matching. Never matching.**

Matching only answers:

“At what price and quantity do two intents cross?”

---

## **Minimum order size — clarified correctly**

With atomic units:

`contract_multiplier = 0.01 BTC`  
`min_order_size = 1 contract`

If later you want:

`min_order_size = 10 contracts (0.1 BTC)`

That’s **OMS validation**, not matching logic.

Matching engine doesn’t reject “small” orders — it never sees invalid ones.

---

# **2️⃣ Cancel & Modify under sequence rules**

This is subtle and extremely important.

---

## **Fundamental rule**

**Cancel and modify are just events.**  
 They are not “actions”, they are *requests* applied in sequence order.

---

## **Cancel order flow (exactly)**

Let’s say:

`Order A accepted at seq=100`  
`Partial fill at seq=120`  
`Cancel request arrives at seq=130`

### **What happens**

* Cancel event is appended at `seq=130`

* Matching engine processes events strictly in seq order

* Any fills **before seq=130** are valid

* Any remaining quantity **after seq=130** is canceled

You never “reach into the past”.

---

## **Modify order (price / quantity)**

There are **two correct models**. Pick one.

### **✅ Model A (recommended): Cancel \+ New**

Modify \= **atomic replacement**

`Cancel(order_id)`  
`New(order_id’, new_price, new_qty)`

Why this is best:

* Preserves determinism

* Preserves price-time priority rules

* Easy to audit

* Matches how real exchanges work

👉 Modified orders **lose time priority**.

---

### **⚠️ Model B: In-place mutation (harder)**

Only allowed if:

* Price does not improve

* Quantity is reduced

Still complex, still risky.

**Do Model A.**

---

## **What if cancel arrives after full fill?**

Nothing happens.

Cancel is a no-op if:

* Order is already FILLED

* Order is already CANCELED

* Order is already EXPIRED

This is why **idempotency matters**.

---

# **3️⃣ OMS ↔ Matching Engine contract**

You had the right intuition earlier — let’s formalize it.

---

## **OMS responsibilities (user-facing brain)**

OMS is **intent management**, not execution.

### **OMS does:**

* Validate order structure

* Validate instrument exists & active

* Validate min order size

* Validate tick size

* Track user-visible order lifecycle

* Assign client\_order\_id

* Submit intent to sequencer

### **OMS does NOT:**

* Match orders

* Decide price crossing

* Allocate fills

* Touch balances

* Decide risk

---

## **Matching engine responsibilities (execution brain)**

Matching engine is **pure and blind**.

It:

* Consumes sequenced events

* Mutates order books

* Emits trades

* Emits order state changes

It does **not**:

* Know user identity

* Know balances

* Know margin

* Reject orders for risk

---

## **The bridge: sequenced events**

The OMS never “calls” the matching engine.

Instead:

`OMS → Sequencer → Event Log → Matching Engine`

This guarantees:

* Ordering

* Replay

* Determinism

* Fault tolerance

---

## **Example flow (limit buy)**

`User submits BUY 10 contracts @ 120 USDT`

`OMS:`  
  `validates`  
  `creates OrderIntent`  
  `sends to sequencer`

`Sequencer:`  
  `assigns seq=501`  
  `appends OrderAccepted event`

`Matching engine:`  
  `processes seq=501`  
  `checks ask ladder`  
  `emits TradeExecuted seq=502`  
  `emits OrderPartiallyFilled seq=503`

OMS simply *observes* these events to update UI.

---

# **4️⃣ Sharded matching engine architecture (done right)**

This is where people accidentally destroy determinism. We won’t.

---

## **First principle**

**One order book \= one writer**

Never violate this.

---

## **Sharding strategy**

Shard by **instrument\_id**.

`Shard 1 → BTC-28MAR2026-50000-C`  
`Shard 2 → BTC-28MAR2026-40000-P`  
`Shard 3 → ETH-28MAR2026-3000-C`

Each shard:

* Has its own in-memory book

* Has its own event queue

* Processes events sequentially

---

## **Global sequencing still applies**

Even with shards:

1. OMS sends intent

2. Global sequencer assigns seq\_no

3. Event routed to shard based on instrument\_id

4. Shard processes events **in seq order for that instrument**

No shard invents its own order.

---

## **Why this is still deterministic**

Because:

* Total ordering exists in the log

* Each shard processes a strict subsequence

* No shared mutable state between shards

* Trade events reference exact seq numbers

Replay still works.

---

## **Scaling knobs you gain**

* Add more shards → more instruments

* No cross-instrument locks

* Near-linear horizontal scale

* Replay per shard

This is how real crypto exchanges scale options.

---

# **🔑 Mental model you should lock in**

* OMS \= **intent & UX**

* Matching \= **pure deterministic state machine**

* Options logic \= **metadata \+ settlement**

* Atomic contracts \= **everything**

* Sequence numbers \= **time**

* Event log \= **truth**

