## **The Plan (high-level, non-negotiable)**

We’ll proceed in **layers**, and we won’t touch code until the architecture is frozen enough to survive scrutiny.

**Phases**

1. **System decomposition** (what *must* exist)

2. **Architecture doc** (services, data flow, invariants)

3. **Core domain specs** (options, margin, risk)

4. **Interfaces & APIs** (internal \+ external)

5. **Ticket breakdown** (engineering-ready)

6. **Only then** → Rust crates & repos

## **Phase 1 — Decompose the Exchange (mental model)**

At minimum, a crypto options exchange has **7 irreducible subsystems**:

### **1\. Market & Instrument Layer**

Defines *what is traded*.

* Underlying assets (BTC, ETH, etc.)

* Option instruments:

  * European style (start here)

  * Expiry

  * Strike

  * Call / Put

* Instrument IDs (canonical, deterministic)

👉 **Invariant**: Instruments are immutable once listed.

### **2\. Order Management System (OMS)**

Handles *intent*.

* Order types:

  * Limit (start here)

  * Market (later)

* Time-in-force

* Order lifecycle:

  * New → Partially filled → Filled / Cancelled / Expired

* User order books (logical view)

👉 **Invariant**: OMS never decides *risk*, only *intent*.

### **3\. Matching Engine**

Handles *price-time priority*.

* Per-instrument order books

* Deterministic matching

* Atomic trades

* No balance logic here

👉 **Invariant**: Given same inputs, matching is deterministic.

### **4\. Risk Engine (MOST IMPORTANT)**

This is where 90% of exchanges fail.

Responsibilities:

* Margin calculation

* Portfolio risk (SPAN-like or simplified)

* Position tracking

* Liquidation eligibility

* Exposure limits

This engine **gates the OMS**.

👉 **Invariant**: No order reaches the matcher unless risk-approved.

### **5\. Clearing & Settlement**

Turns trades into state.

* Position updates

* Margin debits / credits

* PnL calculation

* Funding / expiry settlement

👉 **Invariant**: Trades are append-only events; state is derived.

### **6\. Wallet / Collateral System**

Manages *money*.

* Deposits

* Withdrawals

* Locked vs free balance

* Insurance fund (eventually)

👉 **Invariant**: Risk engine never mutates wallets directly.

### **7\. Market Data & Feeds**

Everything observable.

* Order book snapshots

* Trades

* Mark price

* Index price

* Greeks (derived)

👉 **Invariant**: Market data is *read-only*, never source of truth.

## **Phase 2 — Architecture Doc (Version 0\)**

Now we turn this into a **technical architecture document**.

### **Architectural Style**

**Event-driven, strongly ordered, deterministic core**

* Core engine: **single-writer per instrument**

* Communication: internal events (in-proc first)

* Persistence: **event log first**, state second

## **Data Model (conceptual)**

### **Core entities**

* `User`

* `Account`

* `Instrument`

* `Order`

* `Trade`

* `Position`

* `MarginSnapshot`

* `WalletBalance`

### **Storage approach**

* **Event Log**

  * OrderPlaced

  * OrderMatched

  * TradeExecuted

  * PositionUpdated

  * MarginUpdated

* **Materialized Views**

  * Current order book

  * User positions

  * Wallet balances

This maps *perfectly* to Rust \+ your systems background.

# **Market & Instrument Layer — Full Breakdown**

## **What this system IS responsible for**

* Defining **what can be traded**

* Defining **how it is identified**

* Defining **when it exists and when it dies**

* Providing **static facts** about instruments

# **Market & Instrument Layer — Full Breakdown**

## **What this system IS responsible for**

* Defining **what can be traded**

* Defining **how it is identified**

* Defining **when it exists and when it dies**

* Providing **static facts** about instruments

## **What this system is NOT responsible for**

❌ Pricing  
 ❌ Risk  
 ❌ Margin  
 ❌ Order books  
 ❌ User-specific state

This layer is **pure, deterministic, boring** — and that’s exactly what we want.

## **LEVEL 0 — Primitives (irreducible atoms)**

These are *value types*, no logic, no behavior.

### **Underlying Asset**

`Asset {`  
  `asset_id        // BTC, ETH`  
  `chain           // Bitcoin, Ethereum (future)`  
  `decimals`  
`}`

**Notes**

* Asset ≠ Collateral

* Asset ≠ Wallet balance

* Immutable once defined

### **Currency**

Used for pricing & settlement.

`Currency {`  
  `currency_id     // USD, USDT`  
  `decimals`  
`}`

For v0:

* One settlement currency: **USDT**

## **LEVEL 1 — Market Definition**

A **Market** is a tradeable universe.

### **Market**

`Market {`  
  `market_id`  
  `underlying_asset   // BTC`  
  `settlement_currency // USDT`  
  `market_type        // OPTIONS`  
  `status             // ACTIVE | HALTED | EXPIRED`  
`}`

**Examples**

* `BTC-OPTIONS-USDT`

* `ETH-OPTIONS-USDT`

👉 A market groups instruments but does **not** define them.

## **LEVEL 2 — Instrument Template**

Before individual options, we define *shapes*.

### **OptionStyle**

`OptionStyle = European`

### **OptionType**

`OptionType = Call | Put`

### **ExerciseType**

`ExerciseType = CashSettled   // v0 only`

## **LEVEL 3 — Option Instrument (Core Entity)**

This is the heart.

`OptionInstrument {`  
  `instrument_id        // deterministic hash or canonical string`  
  `market_id`

  `underlying_asset     // BTC`  
  `option_type          // CALL / PUT`  
  `style                // EUROPEAN`

  `strike_price`  
  `expiry_timestamp`

  `contract_size        // e.g. 1 BTC`  
  `settlement_currency  // USDT`

  `tick_size`  
  `min_order_size`

  `status               // LISTED | ACTIVE | EXPIRED | SETTLED`  
  `created_at`  
`}`

### **Design Rules (non-negotiable)**

* **Immutable after LISTED**

* Expiry is absolute time (UTC)

* Strike is in settlement currency

* No implied volatility stored here

## **LEVEL 4 — Instrument Identity & Naming**

This is more important than people realize.

### **Canonical Instrument Name**

Example:

`BTC-28MAR2026-50000-C`  
`BTC-28MAR2026-40000-P`

Rules:

* Deterministic

* Human-readable

* Machine-parseable

👉 `instrument_id = hash(canonical_name)`

## **LEVEL 5 — Instrument Lifecycle**

This is a **state machine**, not logic.

### **States**

`DRAFT → LISTED → ACTIVE → EXPIRED → SETTLED → ARCHIVED`

### **Transitions**

* LISTED → ACTIVE (market opens)

* ACTIVE → EXPIRED (expiry timestamp)

* EXPIRED → SETTLED (settlement done)

**Important**

* Market layer does NOT settle

* It only *declares* expiry

## **LEVEL 6 — Instrument Registry**

This is the actual “system”.

### **Instrument Registry Responsibilities**

* Create instruments

* Validate invariants

* Store immutable metadata

* Expose read-only queries

`InstrumentRegistry {`  
  `list_markets()`  
  `list_instruments(market_id)`  
  `get_instrument(instrument_id)`  
`}`

No writes from outside admin flow.

## **LEVEL 7 — Constraints & Validation**

### **Validation rules (examples)**

* Expiry must be in the future

* Strike \> 0

* Tick size divides price cleanly

* Contract size fixed per market

This logic **lives here**, not in OMS.

## **LEVEL 8 — Time & Calendar Model**

Surprisingly important.

### **Trading Calendar**

`TradingCalendar {`  
  `timezone = UTC`  
  `trading_days = ALL_DAYS   // crypto`  
`}`

### **Expiry Rules**

* Expiry happens at fixed UTC time

* No trading after expiry timestamp

---

## **LEVEL 9 — Read Models / APIs**

What other systems see.

`GET /markets`  
`GET /markets/{id}/instruments`  
`GET /instruments/{id}`

Read-only. Cached aggressively.

---

## **Summary Mental Model**

**Market & Instrument Layer \= ontology \+ law book**

It answers:

* *What exists?*

* *What is it called?*

* *When does it exist?*

* *What are its immutable properties?*

Nothing else.

---

## **Rust-Specific Guidance (important)**

* Use **newtype wrappers** (`StrikePrice`, `ExpiryTs`)

* Enums for state machines

* `serde`\-friendly, but no DB logic here

* Pure structs \+ validation functions

---

## **What’s Next (sequenced properly)**

Next system to break:

1. **Order Management System (OMS)**  
    OR

2. **Risk Engine primitives** (positions, margin units)

My recommendation:  
 👉 **OMS next**, *but* only after we lock the instrument model.

If you want, next step I can:

* Turn this into a **formal spec doc**

* Or write the **Rust domain types**

* Or draw the **state machine diagram in text**

Tell me how you want to proceed.

# **LEVEL 0 — Primitives**

**“Irreducible atoms of meaning”**

*These are value types, no logic, no behavior.*

This sentence is doing a LOT of work. Let’s unpack it first.

### **What this really means**

* These types **encode facts**, not actions

* They are **pure data**, not services

* They should be:

  * Comparable

  * Serializable

  * Hashable

  * Stable forever

Once created, they become **axioms** that the rest of the exchange assumes are true.

Think of them like **units of reality**, not software objects.

## **1\. Underlying Asset**

`Asset {`  
  `asset_id        // BTC, ETH`  
  `chain           // Bitcoin, Ethereum (future)`  
  `decimals`  
`}`

### **What problem this solves**

The exchange needs a **shared agreement** on *what the derivative references*.

When someone trades:

“BTC-28MAR2026-50000-C”

Everyone must mean **the same BTC**.

This struct answers that.

## **1\. Underlying Asset — “What reality are we referencing?”**

### **What it does**

An **underlying asset** answers:

*What thing in the real world does this derivative depend on?*

Example:

* BTC options → underlying \= BTC

### **Why it exists**

Options are **second-order instruments**.  
 You cannot price, settle, or risk-manage them unless *everyone agrees* what the base thing is.

### **Mental invariant**

An option **points to** an asset, it never contains the asset.

This separation lets you later:

* Add futures

* Add indices

* Add synthetic underlyings

## **2\. Currency — “What unit are we measuring value in?”**

### **What it does**

Defines the **unit of account**:

* Prices

* Strikes

* Settlement PnL

### **Why it exists separately**

BTC ≠ USD ≠ USDT

If you don’t separate this cleanly:

* You’ll bake assumptions into pricing

* You’ll break settlement later

### **Mental invariant**

Instruments talk in *assets*, value is expressed in *currency*.

## **3\. Market — “A universe with rules”**

### **What a Market really is**

A **market** is not a book.  
 It’s a *context*:

“In this universe, BTC options exist, priced and settled in USDT.”

### **What it groups**

* Same underlying

* Same settlement currency

* Same trading rules

### **Why this abstraction matters**

Markets let you:

* Halt everything at once

* Version rules later

* Add BTC-OPTIONS-USD without chaos

### **Mental invariant**

Instruments belong to markets; markets define global constraints.

## **4\. Option Template Concepts — “Shapes before instances”**

Before you list an option, you define **what kind of option is even allowed**.

### **Option Type (Call / Put)**

Defines **directional rights**:

* Call → right to buy

* Put → right to sell

### **Option Style (European)**

Defines **when exercise happens**:

* European → only at expiry

This is critical because:

* Risk math depends on it

* Settlement logic depends on it

### **Mental invariant**

Behavior is decided at the *type level*, not per instrument.

###  **Underlying Asset — Conceptually**

* **Purpose:** Represents the “thing in reality” that the derivative (option) depends on.

* **Crucial invariant:** An option only *points to* an underlying asset. It **never contains the asset itself**.

* **Why immutable:** If BTC suddenly meant something else halfway through, all positions, risk, and trades break.

So for v0:

`Assets = BTC, ETH`

This is **all your options need to know** to be meaningful.

* It’s **not collateral**, not a wallet balance, not something you store; it’s a reference frame.

* It allows you to extend the system later to futures, indices, synthetics without touching other layers.

###  **Asset Struct — Technical Perspective**

You are correct: at this layer, it should be a **struct**, not a trait.

* **Why struct, not trait?**

  * Traits define behavior, this is pure data (value type, no logic).

  * Immutable, comparable, serializable — pure facts.

Here’s the Rust-style mental mapping:

\#\[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)\]

pub struct Asset {

    pub asset\_id: String, // "BTC", "ETH"

    pub chain: String,    // "Bitcoin", "Ethereum"

    pub decimals: u8,     // precision

}

Each incoming price, instrument, or reference to BTC/ETH will **point to an `Asset`** instance.

This is the root of your “ontology” for assets.

### **How this interacts with instruments**

* An **option instrument** will have a field like:

`pub struct OptionInstrument {`

    `pub instrument_id: String,`

    `pub underlying: Asset,      // points to BTC or ETH`

    `pub strike_price: u64,      // in settlement currency decimals`

    `pub expiry: u64,            // timestamp UTC`

    `pub option_type: OptionType,`

    `// ...other fields`

`}`

* **Mental invariant:** `OptionInstrument` does **not contain BTC**, only a reference to the asset (struct).

* The **Asset struct** itself is canonical, immutable, and unique per traded asset.

###  **How pricing comes in**

* Yes, live prices **come from an external price feed** (oracle, exchange API).

* You **do not store BTC/ETH**, you **query price** and pass that into the risk engine / margin calculations.

* Your system just needs to know: “BTC is the underlying for this option” — the rest (market price) is **dynamic**, not part of the asset primitive.

### **Summary / Mental Model**

* Asset struct \= canonical truth about the underlying.

* Option points to Asset, nothing more.

* Pricing, margin, balances, collateral → all separate layers.

* Future extensibility: you can add synthetic underlyings or indices by just creating new `Asset` structs.

---

If you want, I can **draw a tiny text diagram** showing **how Asset → OptionInstrument → OMS → Risk → Matching Engine** flows. This often makes the mental architecture click perfectly.

### **Raw asset prices (BTC/ETH)**

* **We do not store BTC or ETH themselves**.

* They exist **only as reference assets** (`Asset` struct).

* **Prices come from external feeds (oracles/exchanges)** — the system queries these live prices when needed:

  * To **calculate option premiums** when a user wants to create an option.

  * To **compute mark-to-market values** for open positions (for reporting, margin, risk).

* The asset struct is immutable, canonical — it never changes because of price moves.

### **Option creation by users**

* When a user creates an option, they define:

  1. **Underlying asset** (`Asset`) — BTC or ETH.

  2. **Strike price** — in settlement currency (USDT).

  3. **Expiry timestamp** — absolute UTC time.

  4. **Option type** — CALL or PUT.

  5. **Premium** — calculated dynamically using current price (from feed) and any pricing logic you implement.

* This **creates a new OptionInstrument instance**, which is **stored in your database**.

`pub struct OptionInstrument {`

    `pub instrument_id: String,      // deterministic ID`

    `pub underlying: Asset,          // BTC / ETH`

    `pub strike_price: u64,          // in USDT`

    `pub expiry_timestamp: u64,      // UTC`

    `pub option_type: OptionType,    // CALL / PUT`

    `pub premium: u64,               // USDT, calculated from feed`

    `pub contract_size: u64,         // e.g. 1 BTC per contract`

    `pub status: OptionStatus,       // ACTIVE / EXPIRED / SETTLED`

`}`

* **This struct is persistent**, because every option ever created is part of the exchange’s immutable history

### **Option lifecycle**

1. **Active / Live:**

   * Option exists in **live storage**.

   * Users can **buy/sell** this option (transfer the premium, enter positions).

   * **Mark price and risk calculations** are updated live using feeds.

2. **Expiry / Settlement:**

   * At expiry timestamp, the option **resolves automatically**:

     * **In-the-money (ITM):** Buyer gains; seller loses (or vice versa, depending on type).

     * **Out-of-the-money (OTM):** Buyer loses only the premium; seller keeps it.

   * This can be automated via your **clearing engine**, which finalizes PnL, updates positions, and moves the option out of live storage.

3. **Historical storage:**

   * Once settled, the option is **archived in permanent storage**.

   * This allows:

     * Auditing

     * Market analytics

     * Historical charts / backtesting

###  **Key mental invariants**

* **OptionInstrument is distinct from Asset.** Asset \= reference, Option \= tradeable contract.

* **Prices never live in OptionInstrument.** Only used to calculate premium and risk.

* **Live vs archived:** Only **active options exist in live order books**; expired options are archived.

* **Resolution is automatic at expiry.** You can also allow early exercise if needed (for American options in future).

**Summary in your own words:**

* You **never store BTC/ETH**, only the reference (`Asset`).

* Users create options based on **live prices from feeds**, generating OptionInstrument structs.

* **Active options** are in live storage for trading; **expired options** move to permanent storage.

* Resolution is automatic; PnL is calculated via the risk/clearing engine.

## **LEVEL 1 — Market Definition**

A **Market** is a tradeable universe.

### **Market**

`Market {`

  `market_id`

  `underlying_asset   // BTC`

  `settlement_currency // USDT`

  `market_type        // OPTIONS`

  `status             // ACTIVE | HALTED | EXPIRED`

`}`

**Examples**

* `BTC-OPTIONS-USDT`

* `ETH-OPTIONS-USDT`

👉 A market groups instruments but does **not** define them.

### **Conceptual Role of Market**

* **Purpose:** A Market **groups instruments** that share the same underlying \+ settlement currency \+ type.

* Think of it like a **container or namespace**:

| Layer | Responsibility |
| ----- | ----- |
| Asset | Canonical reference to real-world object (BTC, ETH) |
| Market | Tradeable universe, defines *what can exist together* (e.g., BTC options in USDT) |
| Instrument | Specific derivative (call/put, strike, expiry) |

* 

* **Important invariant:** A Market **does not create or define instruments** — it only *hosts them*.

* This separation keeps your architecture clean: the **instrument registry** handles instruments, while Market just organizes them.

### **Market Fields Breakdown**

`pub struct Market {`

    `pub market_id: String,            // e.g., "BTC-OPTIONS-USDT"`

    `pub underlying_asset: Asset,      // points to BTC / ETH`

    `pub settlement_currency: Currency, // e.g., USDT`

    `pub market_type: MarketType,      // OPTIONS (future: FUTURES, SWAPS)`

    `pub status: MarketStatus,         // ACTIVE | HALTED | EXPIRED`

`}`

**Field purpose:**

1. `market_id`

   * Deterministic, human-readable canonical ID.

   * Often a combination: `{UNDERLYING}-{TYPE}-{SETTLEMENT}` → e.g., `BTC-OPTIONS-USDT`.

2. `underlying_asset`

   * Links Market to the Asset layer.

   * Ensures all instruments within the market reference the same underlying.

3. `settlement_currency`

   * Defines which currency the option settles in.

   * For v0, USDT only, but easily extendable.

4. `market_type`

   * Currently OPTIONS.

   * Future-proofing for Futures, Swaps, etc.

5. `status`

   * ACTIVE → trading allowed

   * HALTED → temporary stop (e.g., oracle failure, emergency)

   * EXPIRED → market closed, no new instruments

###  **Market vs Instrument — Key Invariants**

* **Market groups instruments**, but instruments are **independent entities**.

Example:

* Market: `BTC-OPTIONS-USDT`

  * Instrument 1: `BTC-28MAR2026-50000-C`

  * Instrument 2: `BTC-28MAR2026-40000-P`

  * Instrument 3: `BTC-28MAR2026-45000-C`

All instruments **live under the same market**, inherit its underlying and settlement currency.

**Why this separation matters:**

* You can **add/remove instruments** without touching the market.

* You can **halt a market**, which affects all instruments in that market uniformly.

* It keeps your **OMS, risk engine, and matching engine simpler**, because they can query by market for grouped operations.

### **Mental Picture**

`Market: BTC-OPTIONS-USDT`

`│`

`├── Instrument: BTC-28MAR2026-50000-C`

`├── Instrument: BTC-28MAR2026-40000-P`

`└── Instrument: BTC-28MAR2026-45000-C`

* Market \= container

* Instruments \= active tradeable contracts

* Each instrument references `underlying_asset = BTC`

###  **Market definition vs expiry/strike**

**Current v0 market definition:**

 `Market = { underlying_asset, settlement_currency, market_type }`

*   
  * BTC-OPTIONS-USDT → includes all BTC options settled in USDT.

  * ETH-OPTIONS-USDT → includes all ETH options settled in USDT.

* **Do we create separate markets for expiry?**

  * **No.** Expiry is a property of the **instrument**, not the market.

  * Market just groups all instruments for that underlying \+ settlement currency.

  * Each instrument has **its own strike and expiry**, which is handled **inside the market**, not as a separate market.

✅ **Invariant:** Market \= grouping container, **instruments differentiate by strike/expiry/type**.

**You do not mix calls and puts in the same bid/ask ladder.**

* Each option instrument (`BTC-28MAR2026-50000-C`) has **its own order book**.

* **Bid \= buyers of this specific instrument**, **Ask \= sellers**.

* Call vs Put → **separate instruments → separate order books**, even if same strike/expiry.

**What people trade:**

* Buyers/sellers trade **existing options**, paying a premium (price \= option premium).

* Secondary market is just **option contracts being transferred**; nothing to do with BTC itself.

* People rarely “exercise early” in European options — they mostly **trade the options on the exchange**.

### **Strike prices and bid/ask ladders**

* Different strike prices → different instruments → **different order books**.

So even for the same expiry:

 `Market: BTC-OPTIONS-USDT`

`├── Instrument: BTC-28MAR2026-50000-C → its own order book`

`├── Instrument: BTC-28MAR2026-55000-C → its own order book`

`├── Instrument: BTC-28MAR2026-50000-P → its own order book`

`└── Instrument: BTC-28MAR2026-55000-P → its own order book`

*   
* **Bid/Ask ladder \= instrument-specific**, not per market.

* Market is just a **namespace for all instruments**.

### **Trading flow — mental picture**

1. User wants to buy a call: BTC-28MAR2026-50000-C.

2. They see **premium** (price of that option), derived from oracle price, volatility, etc.

3. Place **limit or market order** → OMS → matching engine → PnL/risk updated.

4. If they want to sell to another user → same order book.

5. Option resolves at expiry → clearing settles PnL.

In short: **the exchange never deals with BTC directly**, only option contracts and their premiums.

###  **Key invariants**

* Market \= grouping, not bid/ask ladder.

* Instrument \= one order book per strike/expiry/type.

* Premium \= price that changes, not the underlying asset.

* Secondary trading \= standard, and **European options are mostly traded this way**, not by exercising immediately.

## **Market Struct**

Market \= tradeable universe. Immutable metadata.

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum MarketStatus {`

    `Active,`

    `Halted,`

    `Expired,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum MarketType {`

    `Options,`

    `// Future-proofing: Futures, Swaps`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct Market {`

    `pub market_id: String,           // BTC-OPTIONS-USDT`

    `pub underlying_asset: Asset,     // BTC / ETH`

    `pub settlement_currency: Currency, // USDT`

    `pub market_type: MarketType,     // OPTIONS`

    `pub status: MarketStatus,        // ACTIVE/HALTED/EXPIRED`

`}`

**Notes / Invariants:**

* Immutable after creation.

* Contains **all instruments** logically under it, but instruments stored separately.

---

## **2️⃣ Instrument Struct**

Each option \= separate order book. Core entity.

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionType {`

    `Call,`

    `Put,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionStatus {`

    `Listed,`

    `Active,`

    `Expired,`

    `Settled,`

    `Archived,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct OptionInstrument {`

    `pub instrument_id: String,       // deterministic hash or canonical string`

    `pub market_id: String,           // parent market`

    `pub underlying: Asset,           // reference asset`

    `pub option_type: OptionType,     // CALL / PUT`

    `pub strike_price: u64,           // in settlement currency decimals`

    `pub expiry_timestamp: u64,       // UTC`

    `pub contract_size: u64,          // e.g., 1 BTC per contract`

    `pub premium: u64,                // in USDT, calculated from oracle`

    `pub status: OptionStatus,        // Listed, Active, Expired, Settled`

    `pub tick_size: u64,              // minimum price step`

    `pub min_order_size: u64,         // minimum contract size`

`}`

**Notes:**

* Each instrument has **its own order book**.

* Strike price, expiry, call/put differentiate it from other instruments.

* Immutable fields: `instrument_id`, `market_id`, `underlying`, `strike_price`, `expiry_timestamp`.

---

## **3️⃣ Order Book**

* Separate per instrument. Tracks bid/ask levels.

`use std::collections::BTreeMap;`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OrderSide {`

    `Buy,`

    `Sell,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OrderType {`

    `Limit,`

    `Market, // future`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OrderStatus {`

    `New,`

    `PartiallyFilled,`

    `Filled,`

    `Cancelled,`

    `Expired,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct Order {`

    `pub order_id: String,`

    `pub instrument_id: String,   // which option instrument`

    `pub user_id: String,`

    `pub side: OrderSide,         // buy/sell`

    `pub order_type: OrderType,`

    `pub price: u64,              // USDT`

    `pub quantity: u64,           // contracts`

    `pub status: OrderStatus,`

    `pub timestamp: u64,          // UTC`

`}`

`// Order book: bid and ask trees, sorted by price`

`#[derive(Debug, Default)]`

`pub struct OrderBook {`

    `pub bids: BTreeMap<u64, Vec<Order>>, // price -> orders`

    `pub asks: BTreeMap<u64, Vec<Order>>, // price -> orders`

`}`

**Notes:**

* `OrderBook` is **per instrument**.

* Bids \= descending, Asks \= ascending for matching engine.

* Only **premium prices** are traded, underlying never moves.

---

## **4️⃣ Trade / Match**

* Each executed trade is append-only.

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct Trade {`

    `pub trade_id: String,`

    `pub instrument_id: String,`

    `pub buyer_id: String,`

    `pub seller_id: String,`

    `pub price: u64,       // executed premium`

    `pub quantity: u64,    // contracts`

    `pub timestamp: u64,   // UTC`

`}`

* Trades feed **risk engine** and **wallet updates**.

* Deterministic and append-only → perfect for event-sourcing.

---

## **5️⃣ Summary of Relationships**

`Asset`

 `│`

 `▼`

`Market ────> contains many instruments`

 `│`

 `▼`

`OptionInstrument ──> has its own OrderBook`

 `│`

 `▼`

`Orders (buy/sell) ──> matched into Trades`

 `│`

 `▼`

`Risk Engine / Clearing / Wallets`

---

### **6️⃣ Mental / Rust design principles**

1. **Structs for data / value types:** `Asset`, `Market`, `OptionInstrument`, `Order`, `Trade`.

2. **Enums for categorical / state:** `OptionType`, `OptionStatus`, `MarketStatus`, `OrderSide`, `OrderType`, `OrderStatus`.

3. **Immutable primitives wherever possible** → easier reasoning in risk engine.

4. **Order book per instrument** → allows clean bid/ask matching.

5. **Event-driven append-only system:** Orders → Trades → Position / Margin snapshots.

##  **Conceptual role of Instrument Template**

* **Purpose:** Capture the **common characteristics shared by all options** in the exchange.

* Think of it like a **class blueprint** in OOP, but **pure data/value types in Rust**.

* This is **immutable metadata** that every actual option will reference.

* Allows consistency and prevents duplication across instruments.

**Key invariants:**

1. All options in v0 are **European style**.

2. Options are **either Call or Put**.

3. Exercise is **cash-settled** — no physical delivery of BTC/ETH.

## **Rust Types for Instrument Template**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionStyle {`

    `European,`

    `// Future: American, Bermudan`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionType {`

    `Call,`

    `Put,`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum ExerciseType {`

    `CashSettled,`

    `// Future: PhysicalDelivery`

`}`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct InstrumentTemplate {`

    `pub style: OptionStyle,         // European`

    `pub option_type: OptionType,    // Call / Put`

    `pub exercise_type: ExerciseType, // CashSettled`

`}`

**Notes / Mental Model:**

* This template is **not an actual tradeable instrument**.

* It defines the **shape that all OptionInstrument instances will conform to**.

* You can use it to **generate instruments programmatically**, e.g., for each strike/expiry combination.

##  **How it fits with Market \+ Instrument**

`Market (BTC-OPTIONS-USDT)`

`│`

`├── Instrument Template: European Call, CashSettled`

`├── Instrument Template: European Put, CashSettled`

`│`

`├── Actual OptionInstrument: BTC-28MAR2026-50000-C → references Template`

`├── Actual OptionInstrument: BTC-28MAR2026-40000-P → references Template`

* **Templates reduce duplication**: you don’t store style/exercise info in every single option if you don’t want.

* They also **standardize logic in OMS/Risk Engine**, because engine knows how European/cash-settled options behave.

---

## **4️⃣ Example usage in Rust**

`let call_template = InstrumentTemplate {`

    `style: OptionStyle::European,`

    `option_type: OptionType::Call,`

    `exercise_type: ExerciseType::CashSettled,`

`};`

`let put_template = InstrumentTemplate {`

    `style: OptionStyle::European,`

    `option_type: OptionType::Put,`

    `exercise_type: ExerciseType::CashSettled,`

`};`

* Later, when you **create actual instruments**, you can reference this template:

`let option_instrument = OptionInstrument {`

    `instrument_id: "BTC-28MAR2026-50000-C".to_string(),`

    `market_id: "BTC-OPTIONS-USDT".to_string(),`

    `underlying: btc_asset.clone(),`

    `strike_price: 50_000_00, // in USDT decimals`

    `expiry_timestamp: 1711603200, // UTC`

    `option_type: call_template.option_type,`

    `status: OptionStatus::Listed,`

    `contract_size: 1,`

    `premium: 2000, // example`

    `tick_size: 10,`

    `min_order_size: 1,`

`};`

## **LEVEL 3 — Option Instrument (Core Entity)**

This is the heart.

`OptionInstrument {`

  `instrument_id        // deterministic hash or canonical string`

  `market_id`

  `underlying_asset     // BTC`

  `option_type          // CALL / PUT`

  `style                // EUROPEAN`

  `strike_price`

  `expiry_timestamp`

  `contract_size        // e.g. 1 BTC`

  `settlement_currency  // USDT`

  `tick_size`

  `min_order_size`

  `status               // LISTED | ACTIVE | EXPIRED | SETTLED`

  `created_at`

`}`

### **Design Rules (non-negotiable)**

* **Immutable after LISTED**

* Expiry is absolute time (UTC)

* Strike is in settlement currency

* No implied volatility stored here

## **Conceptual Role of OptionInstrument**

* **Purpose:** Represents a single **tradeable option contract**.

* **Key invariants / rules:**

  * **Immutable after LISTED** — strike, expiry, underlying, style cannot change.

  * **Expiry is absolute UTC** — simplifies matching and risk.

  * **Strike price denominated in settlement currency** (USDT).

  * **No implied volatility stored** here — volatility is derived externally (for pricing / risk).

* **Relation to other layers:**

  * References a **Market** (groups instruments).

  * References an **Asset** (underlying).

  * Option type, style, exercise rules can reference the **InstrumentTemplate**.

---

## **2️⃣ Fields Breakdown**

| Field | Type / Example | Notes |
| ----- | ----- | ----- |
| `instrument_id` | String / deterministic hash `"BTC-28MAR2026-50000-C"` | Canonical identifier, must be unique. |
| `market_id` | String | Parent market (`BTC-OPTIONS-USDT`) |
| `underlying_asset` | Asset | Reference to the underlying BTC/ETH |
| `option_type` | OptionType | Call / Put |
| `style` | OptionStyle | European |
| `strike_price` | u64 | In settlement currency (USDT) decimals |
| `expiry_timestamp` | u64 | UTC timestamp |
| `contract_size` | u64 | e.g., 1 BTC per contract |
| `settlement_currency` | Currency | USDT |
| `tick_size` | u64 | Minimum price increment (for order placement) |
| `min_order_size` | u64 | Minimum number of contracts per order |
| `status` | OptionStatus | Listed / Active / Expired / Settled |
| `created_at` | u64 | UTC timestamp of creation |

---

## **3️⃣ Rust Struct Representation**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct OptionInstrument {`

    `pub instrument_id: String,       // unique, deterministic`

    `pub market_id: String,           // parent market`

    `pub underlying_asset: Asset,     // BTC / ETH`

    `pub option_type: OptionType,     // Call / Put`

    `pub style: OptionStyle,          // European`

    `pub strike_price: u64,           // in USDT decimals`

    `pub expiry_timestamp: u64,       // UTC`

    `pub contract_size: u64,          // e.g., 1 BTC per contract`

    `pub settlement_currency: Currency, // USDT`

    `pub tick_size: u64,              // price increment`

    `pub min_order_size: u64,         // minimum contracts per order`

    `pub status: OptionStatus,        // Listed / Active / Expired / Settled`

    `pub created_at: u64,             // UTC`

`}`

---

## **4️⃣ Key Design Invariants / Mental Model**

1. **Immutable after LISTED**

   * Once `status = Listed`, fields like `strike_price`, `expiry_timestamp`, `underlying_asset` cannot be changed.

   * Ensures **deterministic matching and risk calculations**.

2. **Expiry is absolute**

   * No relative expiry, no trading past expiry.

   * Simplifies OMS, risk engine, clearing.

3. **Strike in settlement currency**

   * All pricing, PnL, and margin calculations are in a **single consistent denomination** (v0 \= USDT).

4. **No implied volatility stored**

   * Keeps instrument lightweight.

   * Volatility is **input to risk engine / pricing**, not part of the instrument’s canonical state.

**Status as state machine**

 `LISTED → ACTIVE → EXPIRED → SETTLED → ARCHIVED`

5.   
   * OMS/matching engine only works with `ACTIVE` instruments.

   * Expiry triggers automated transition to `EXPIRED` → then `SETTLED` after clearing.

---

## **5️⃣ How it Fits in the Architecture**

`Asset (BTC)  ──┐`

                `│`

`Market (BTC-OPTIONS-USDT) ──┐`

                             `│`

`InstrumentTemplate (European Call, CashSettled)`

                             `│`

`OptionInstrument ──> OrderBook ──> Orders → Trades`

* Market groups instruments.

* Template defines shape/behavior.

* OptionInstrument is **actual tradeable entity** with strike, expiry, and premium.

---

## **6️⃣ Example**

`let btc_asset = Asset {`

    `asset_id: "BTC".to_string(),`

    `chain: "Bitcoin".to_string(),`

    `decimals: 8,`

`};`

`let usdt = Currency {`

    `currency_id: "USDT".to_string(),`

    `decimals: 6,`

`};`

`let option_instrument = OptionInstrument {`

    `instrument_id: "BTC-28MAR2026-50000-C".to_string(),`

    `market_id: "BTC-OPTIONS-USDT".to_string(),`

    `underlying_asset: btc_asset.clone(),`

    `option_type: OptionType::Call,`

    `style: OptionStyle::European,`

    `strike_price: 50_000_00,       // 50,000 USDT`

    `expiry_timestamp: 1711603200,   // UTC`

    `contract_size: 1,               // 1 BTC`

    `settlement_currency: usdt,`

    `tick_size: 10,`

    `min_order_size: 1,`

    `status: OptionStatus::Listed,`

    `created_at: 1700000000,        // example UTC`

`};`

## **LEVEL 4 — Instrument Identity & Naming**

This is more important than people realize.

### **Canonical Instrument Name**

Example:

`BTC-28MAR2026-50000-C`

`BTC-28MAR2026-40000-P`

Rules:

* Deterministic

* Human-readable

* Machine-parseable

👉 `instrument_id = hash(canonical_name)`

## **Conceptual Role**

* **Purpose:** Give every OptionInstrument a **unique, deterministic, human-readable identifier**.

* **Why it matters:**

  1. OMS → matches orders correctly.

  2. Matching Engine → deterministic trades.

  3. Risk Engine → calculates PnL, margin, and exposure accurately.

  4. Event Sourcing / persistence → every trade references the same canonical ID.

* **Rule of thumb:** Treat `instrument_id` as **the single source of truth** for an option.

---

## **2️⃣ Canonical Name Structure**

**Pattern (example for BTC options):**

`{UNDERLYING}-{EXPIRY}-{STRIKE}-{TYPE}`

**Example:**

| Component | Example Value | Notes |
| ----- | ----- | ----- |
| UNDERLYING | BTC | Matches Asset.asset\_id |
| EXPIRY | 28MAR2026 | UTC date of expiry, formatted human-readable |
| STRIKE | 50000 | Strike price in settlement currency |
| TYPE | C / P | Call or Put |

**Resulting canonical name:** `BTC-28MAR2026-50000-C`

* Call \= `C`

* Put \= `P`

**Notes:**

* **Human-readable:** Easy for users / UI / reporting.

* **Machine-parseable:** Can be split by `-` to get underlying, expiry, strike, type.

* **Deterministic:** Same input → same name → same hash → same instrument\_id.

---

## **3️⃣ Deterministic ID**

* **`instrument_id` \= hash(canonical\_name)\`**

`use sha2::{Sha256, Digest};`

`fn generate_instrument_id(canonical_name: &str) -> String {`

    `let mut hasher = Sha256::new();`

    `hasher.update(canonical_name.as_bytes());`

    `let result = hasher.finalize();`

    `hex::encode(result)`

`}`

**Why hash instead of using the canonical name directly?**

1. Fixed length → storage-friendly.

2. Prevents errors if UI or external feed sends slightly different string.

3. Guarantees uniqueness across the system.

---

## **4️⃣ Rust Representation**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct OptionInstrumentId(String);`

`impl OptionInstrumentId {`

    `pub fn from_canonical_name(canonical_name: &str) -> Self {`

        `let mut hasher = Sha256::new();`

        `hasher.update(canonical_name.as_bytes());`

        `let hash = hasher.finalize();`

        `Self(hex::encode(hash))`

    `}`

`}`

* Canonical name is still stored in `OptionInstrument` for **UI / reporting**.

* `instrument_id` is used in **all internal systems** for deterministic references.

---

## **5️⃣ Example Usage**

`let canonical_name = "BTC-28MAR2026-50000-C";`

`let instrument_id = OptionInstrumentId::from_canonical_name(canonical_name);`

`let option = OptionInstrument {`

    `instrument_id: instrument_id.0.clone(),`

    `market_id: "BTC-OPTIONS-USDT".to_string(),`

    `underlying_asset: btc_asset.clone(),`

    `option_type: OptionType::Call,`

    `style: OptionStyle::European,`

    `strike_price: 50_000_00,`

    `expiry_timestamp: 1711603200,`

    `contract_size: 1,`

    `settlement_currency: usdt,`

    `tick_size: 10,`

    `min_order_size: 1,`

    `status: OptionStatus::Listed,`

    `created_at: 1700000000,`

`};`

* **UI can show canonical name:** `BTC-28MAR2026-50000-C`.

* **Engine works with instrument\_id hash** → deterministic, fast lookup.

---

## **6️⃣ Key Principles / Takeaways**

1. **Deterministic** → same inputs always produce same instrument.

2. **Human-readable** → for reporting and debugging.

3. **Machine-parseable** → OMS / Risk Engine / Feeds can extract underlying, expiry, strike, type programmatically.

4. **instrument\_id \= hash(canonical\_name)** → internal source of truth, fixed length, immutable.

5. **Everything downstream references instrument\_id** → trades, orders, positions, risk, wallet balances.

## **LEVEL 5 — Instrument Lifecycle**

This is a **state machine**, not logic.

### **States**

`DRAFT → LISTED → ACTIVE → EXPIRED → SETTLED → ARCHIVED`

### **Transitions**

* LISTED → ACTIVE (market opens)

* ACTIVE → EXPIRED (expiry timestamp)

* EXPIRED → SETTLED (settlement done)

**Important**

* Market layer does NOT settle

* It only *declares* expiry

## **Conceptual Role**

* **Purpose:** Track the **state of an OptionInstrument** over time.

* **Important principle:** This is a **state machine**, not logic — it only defines **valid states and transitions**.

* **Why it matters:**

  1. OMS only interacts with **ACTIVE instruments**.

  2. Matching engine only matches orders for **ACTIVE instruments**.

  3. Risk engine uses expiry to trigger **settlement eligibility**.

  4. Market layer simply declares expiry; clearing handles settlement.

---

## **2️⃣ States**

| State | Description |
| ----- | ----- |
| DRAFT | Instrument is created but not yet listed. Internal prep only. |
| LISTED | Instrument officially listed in market, immutable fields set, ready for trading when market opens. |
| ACTIVE | Market is open; instrument accepts orders, trades occur. |
| EXPIRED | Expiry timestamp reached; instrument stops trading. |
| SETTLED | PnL has been calculated and balances updated; risk cleared. |
| ARCHIVED | Historical record; no longer relevant for active trading or matching. |

**Key principle:**

* **Instrument cannot jump states arbitrarily**. Only valid transitions allowed.

---

## **3️⃣ Transitions**

| From | To | Trigger / Notes |
| ----- | ----- | ----- |
| DRAFT | LISTED | Admin approves / listing flow completed. |
| LISTED | ACTIVE | Market opens for trading. |
| ACTIVE | EXPIRED | Expiry timestamp reached. |
| EXPIRED | SETTLED | Clearing engine calculates final PnL and updates wallets. |
| SETTLED | ARCHIVED | Optional archival for historical storage. |

Important: **Market layer never settles.**  
 It only declares expiry → clearing/settlement handled separately.

---

## **4️⃣ Rust Representation**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionStatus {`

    `Draft,`

    `Listed,`

    `Active,`

    `Expired,`

    `Settled,`

    `Archived,`

`}`

`impl OptionStatus {`

    `// Checks if a transition is valid`

    `pub fn can_transition_to(&self, next: &OptionStatus) -> bool {`

        `match self {`

            `OptionStatus::Draft => matches!(next, OptionStatus::Listed),`

            `OptionStatus::Listed => matches!(next, OptionStatus::Active),`

            `OptionStatus::Active => matches!(next, OptionStatus::Expired),`

            `OptionStatus::Expired => matches!(next, OptionStatus::Settled),`

            `OptionStatus::Settled => matches!(next, OptionStatus::Archived),`

            `OptionStatus::Archived => false,`

        `}`

    `}`

`}`

**Notes:**

* Each OptionInstrument has a `status: OptionStatus`.

* **OMS checks status \= ACTIVE** before accepting orders.

* **Expiry events** automatically move ACTIVE → EXPIRED.

* **Clearing engine** then moves EXPIRED → SETTLED.

* Optional archival moves SETTLED → ARCHIVED.

---

## **5️⃣ Lifecycle Flow (Visual / Text)**

`DRAFT`

  `│ (listing approved)`

  `▼`

`LISTED`

  `│ (market opens)`

  `▼`

`ACTIVE`

  `│ (expiry timestamp reached)`

  `▼`

`EXPIRED`

  `│ (clearing / settlement)`

  `▼`

`SETTLED`

  `│ (optional archival)`

  `▼`

`ARCHIVED`

* **DRAFT → LISTED**: pre-listing internal prep.

* **LISTED → ACTIVE**: trading starts.

* **ACTIVE → EXPIRED**: stops trading.

* **EXPIRED → SETTLED**: PnL calculated, wallets updated.

* **SETTLED → ARCHIVED**: long-term storage, historical reference.

---

## **6️⃣ Integration Notes**

* OMS / Matching Engine → only ACTIVE instruments.

* Risk Engine → tracks EXPIRED instruments for settlement.

* Market Layer → only declares expiry; does **not** settle positions.

* Instrument fields like strike, underlying, tick size → immutable after LISTED.

## **LEVEL 6 — Instrument Registry**

This is the actual “system”.

### **Instrument Registry Responsibilities**

* Create instruments

* Validate invariants

* Store immutable metadata

* Expose read-only queries

`InstrumentRegistry {`

  `list_markets()`

  `list_instruments(market_id)`

  `get_instrument(instrument_id)`

`}`

No writes from outside admin flow.

##  **Conceptual Role**

* **Purpose:** Central authoritative system for **all OptionInstruments and Markets**.

* **Responsibilities:**

  1. **Create instruments** — admin flow only.

  2. **Validate invariants** — strike \> 0, expiry in future, tick divides strike cleanly, contract size \> 0, immutable fields.

  3. **Store immutable metadata** — canonical names, underlying asset, market, option type, style, etc.

  4. **Expose read-only queries** — OMS, Matching Engine, Risk Engine, UI all query instruments here.

* **Important:** **No writes from outside admin flow** → instruments are immutable once listed.

---

## **2️⃣ Responsibilities in Detail**

| Responsibility | Description |
| ----- | ----- |
| Create Instruments | Admin defines instruments: strike, expiry, type, style, contract size. |
| Validate Invariants | Checks that all rules of OptionInstrument are satisfied before adding. |
| Store Immutable Metadata | Keeps OptionInstrument structs in persistent store (DB or in-memory). |
| Read-only Queries | Expose list of markets, instruments by market, or individual instrument by ID. |

---

## **3️⃣ Rust Representation**

`use std::collections::HashMap;`

`#[derive(Debug, Default)]`

`pub struct InstrumentRegistry {`

    `// market_id -> vector of instruments`

    `pub instruments_by_market: HashMap<String, Vec<OptionInstrument>>,`

    `// instrument_id -> instrument`

    `pub instruments_by_id: HashMap<String, OptionInstrument>,`

    `// markets`

    `pub markets: HashMap<String, Market>,`

`}`

`impl InstrumentRegistry {`

    `// Admin flow: create a new market`

    `pub fn add_market(&mut self, market: Market) {`

        `self.markets.insert(market.market_id.clone(), market);`

    `}`

    `// Admin flow: add a new instrument`

    `pub fn add_instrument(&mut self, instrument: OptionInstrument) -> Result<(), String> {`

        `// Validate invariants`

        `if instrument.strike_price == 0 {`

            `return Err("Strike price must be > 0".to_string());`

        `}`

        `if instrument.contract_size == 0 {`

            `return Err("Contract size must be > 0".to_string());`

        `}`

        `if !self.markets.contains_key(&instrument.market_id) {`

            `return Err("Market does not exist".to_string());`

        `}`

        `let market_instruments = self.instruments_by_market`

            `.entry(instrument.market_id.clone())`

            `.or_insert_with(Vec::new);`

        `market_instruments.push(instrument.clone());`

        `self.instruments_by_id.insert(instrument.instrument_id.clone(), instrument);`

        `Ok(())`

    `}`

    `// Read-only queries`

    `pub fn list_markets(&self) -> Vec<&Market> {`

        `self.markets.values().collect()`

    `}`

    `pub fn list_instruments(&self, market_id: &str) -> Vec<&OptionInstrument> {`

        `self.instruments_by_market`

            `.get(market_id)`

            `.map(|v| v.iter().collect())`

            `.unwrap_or_default()`

    `}`

    `pub fn get_instrument(&self, instrument_id: &str) -> Option<&OptionInstrument> {`

        `self.instruments_by_id.get(instrument_id)`

    `}`

`}`

---

## **4️⃣ Key Principles / Invariants**

1. **Immutability:** Once an instrument is `LISTED`, fields cannot change. Registry enforces this.

2. **Centralized Read Access:** All OMS, matching engine, risk engine, and UI query instruments here.

3. **Validation:** Ensures deterministic, consistent instruments before exposure.

4. **Admin-only writes:** No user can create or mutate instruments — prevents inconsistencies.

5. **Mapping:**

   * `market_id -> instruments` → easy lookup per market.

   * `instrument_id -> instrument` → fast deterministic lookup for OMS & trades.

---

## **5️⃣ Example Usage**

`let mut registry = InstrumentRegistry::default();`

`// Add market`

`let btc_market = Market {`

    `market_id: "BTC-OPTIONS-USDT".to_string(),`

    `underlying_asset: btc_asset.clone(),`

    `settlement_currency: usdt.clone(),`

    `market_type: MarketType::Options,`

    `status: MarketStatus::Active,`

`};`

`registry.add_market(btc_market);`

`// Add instrument`

`registry.add_instrument(option_instrument.clone()).unwrap();`

`// Queries`

`let btc_options = registry.list_instruments("BTC-OPTIONS-USDT");`

`let instrument = registry.get_instrument(&option_instrument.instrument_id);`

---

## **6️⃣ Integration with the rest of the system**

`User/OMS/Matching Engine/Risk Engine → queries → InstrumentRegistry`

`Admin flow → writes → InstrumentRegistry → validates & stores`

* **OMS:** asks registry → is instrument ACTIVE? → yes → accepts order

* **Matching Engine:** queries registry → gets instrument metadata → matches orders

* **Risk Engine:** queries registry → uses strike, expiry, type for PnL / margin

* **UI:** queries registry → displays canonical name, expiry, strike, call/put

## **Conceptual Role**

* **Purpose:** Ensure **all instruments conform to immutable, non-negotiable rules** before being listed or activated.

* **Why not in OMS?**

  * OMS handles **orders / intents**, not instrument correctness.

  * Keeping validation separate keeps **clear separation of concerns**.

  * Instrument validation is **pre-trade system integrity**; OMS is **trade-time logic**.

* **Who uses this?**

  * **Instrument Registry** → calls validation when adding instruments.

  * Admin flow → uses validation before listing an instrument.

---

## **2️⃣ Example Constraints / Rules**

| Rule | Description |
| ----- | ----- |
| Expiry must be in the future | Prevents creating instruments that are already expired. |
| Strike \> 0 | Ensures non-zero, meaningful strike price. |
| Tick size divides price cleanly | Price increments must align with tick size (no fractional ticks). |
| Contract size fixed per market | Ensures uniformity; all instruments in same market have same contract size. |

---

## **3️⃣ Rust Implementation Example**

`impl OptionInstrument {`

    `pub fn validate(&self, market_contract_size: u64) -> Result<(), String> {`

        `// Expiry in the future`

        `let now = chrono::Utc::now().timestamp() as u64;`

        `if self.expiry_timestamp <= now {`

            `return Err("Expiry must be in the future".to_string());`

        `}`

        `// Strike price > 0`

        `if self.strike_price == 0 {`

            `return Err("Strike price must be > 0".to_string());`

        `}`

        `// Tick size divides strike cleanly`

        `if self.strike_price % self.tick_size != 0 {`

            `return Err("Strike price must be divisible by tick size".to_string());`

        `}`

        `// Contract size fixed per market`

        `if self.contract_size != market_contract_size {`

            `return Err("Contract size must match market standard".to_string());`

        `}`

        `Ok(())`

    `}`

`}`

---

## **4️⃣ How it Fits in the Flow**

`Admin creates OptionInstrument → validate() → add to InstrumentRegistry`

      `│`

      `└─> Validation passes? Yes → Listed / Active eventually`

      `└─> Validation fails? No → Reject`

* **OMS never validates strike/tick/expiry** — it trusts the registry.

* **Matching Engine** only sees **valid instruments**.

* **Risk Engine** can assume **expiry / strike / contract size** are always consistent.

---

## **5️⃣ Notes / Mental Model**

1. Validation \= **pre-trade system safety**.

2. Immutable, deterministic rules → **no surprises in OMS / Matching / Risk**.

3. Helps **future-proof**: if you add American/Bermudan options or futures, you just extend validation rules here.

4. This is **separate from runtime rules** (like order size, margin checks, exposures).

##  **Conceptual Role**

* **Purpose:** Standardize **time handling** across all layers: OMS, Matching Engine, Risk Engine, and Clearing.

* Crypto markets are **24/7**, but we still need:

  * Fixed expiry timestamps (UTC) for instruments.

  * Clear rules for **when trading stops**.

  * Consistent scheduling for clearing/settlement.

* **Why it matters:**

  * Avoids race conditions around expiry.

  * Ensures **deterministic behavior** in all systems.

  * Makes event sourcing and historical analysis reliable.

---

## **2️⃣ Trading Calendar**

`#[derive(Debug, Clone)]`

`pub struct TradingCalendar {`

    `pub timezone: chrono::FixedOffset,  // UTC`

    `pub trading_days: Vec<chrono::Weekday>, // For crypto, all days included`

`}`

`impl Default for TradingCalendar {`

    `fn default() -> Self {`

        `Self {`

            `timezone: chrono::FixedOffset::east(0), // UTC`

            `trading_days: vec![`

                `chrono::Weekday::Mon,`

                `chrono::Weekday::Tue,`

                `chrono::Weekday::Wed,`

                `chrono::Weekday::Thu,`

                `chrono::Weekday::Fri,`

                `chrono::Weekday::Sat,`

                `chrono::Weekday::Sun,`

            `],`

        `}`

    `}`

`}`

**Notes:**

* Crypto is 24/7 → `trading_days = ALL_DAYS`.

* Futures or equities may have restricted calendars → easy to extend.

* All times normalized to **UTC** to avoid timezone bugs.

---

## **3️⃣ Expiry Rules**

* **Expiry happens at fixed UTC timestamp**:

  * Each OptionInstrument has `expiry_timestamp: u64` (seconds since epoch).

  * Deterministic; no ambiguity about expiry.

* **No trading allowed after expiry timestamp**:

  * OMS rejects orders with `instrument.status = EXPIRED`.

  * Matching engine only matches orders **before expiry**.

* **Automated transition**:

  * Cron/job/event scheduler marks ACTIVE → EXPIRED at expiry.

  * Clearing engine can then settle the instrument.

---

## **4️⃣ Rust Helper Functions**

`impl OptionInstrument {`

    `pub fn has_expired(&self, current_ts: u64) -> bool {`

        `current_ts >= self.expiry_timestamp`

    `}`

`}`

`impl TradingCalendar {`

    `pub fn is_trading_day(&self, timestamp: chrono::DateTime<chrono::Utc>) -> bool {`

        `self.trading_days.contains(&timestamp.weekday())`

    `}`

`}`

**Usage:**

* OMS checks `has_expired()` before accepting orders.

* Scheduler triggers **state transition** from ACTIVE → EXPIRED.

* Risk engine uses expiry timestamps to calculate **settlement eligibility**.

---

## **5️⃣ Integration in the Architecture**

`OptionInstrument → expiry_timestamp (UTC)`

        `│`

        `▼`

`TradingCalendar → checks allowed trading days & times`

        `│`

        `▼`

`OMS / Matching Engine → reject orders if expired`

        `│`

        `▼`

`Scheduler → triggers ACTIVE → EXPIRED → SETTLED`

* Ensures **all components use the same deterministic time reference**.

* Avoids bugs with timezones or late orders at expiry.

* Makes **event sourcing / auditing** reliable.

## **Conceptual Role**

* **Purpose:** Expose **markets, instruments, and related metadata** to internal and external systems.

* **Key principle:** **Read-only, cached, fast**. Core exchange state is **never mutated through APIs**.

* **Why it matters:**

  1. OMS & Matching Engine need **instrument metadata** to validate orders.

  2. Risk Engine needs **strike, expiry, contract size** to calculate PnL and margin.

  3. UI / frontend needs **human-readable names, expiry, strike, option type** for users.

  4. Analytics and monitoring systems need consistent access without affecting production state.

---

## **2️⃣ Core Endpoints / Read Models**

| Endpoint | Returns | Notes |
| ----- | ----- | ----- |
| `GET /markets` | List of all markets | e.g., `BTC-OPTIONS-USDT`, `ETH-OPTIONS-USDT` |
| `GET /markets/{id}/instruments` | List of OptionInstruments in a market | Only ACTIVE / LISTED instruments usually, optionally all states |
| `GET /instruments/{id}` | Single OptionInstrument by `instrument_id` | Canonical metadata, human-readable name, status, strike, expiry, etc. |

**Read-only**: No endpoint allows modifying instruments or markets.

---

## **3️⃣ Rust Representation**

`#[derive(Debug, Serialize)]`

`pub struct MarketResponse {`

    `pub market_id: String,`

    `pub underlying_asset: Asset,`

    `pub settlement_currency: Currency,`

    `pub market_type: MarketType,`

    `pub status: MarketStatus,`

`}`

`#[derive(Debug, Serialize)]`

`pub struct InstrumentResponse {`

    `pub instrument_id: String,`

    `pub canonical_name: String,`

    `pub market_id: String,`

    `pub underlying_asset: Asset,`

    `pub option_type: OptionType,`

    `pub style: OptionStyle,`

    `pub strike_price: u64,`

    `pub expiry_timestamp: u64,`

    `pub contract_size: u64,`

    `pub settlement_currency: Currency,`

    `pub tick_size: u64,`

    `pub min_order_size: u64,`

    `pub status: OptionStatus,`

    `pub created_at: u64,`

`}`

`// Example API function`

`impl InstrumentRegistry {`

    `pub fn get_markets(&self) -> Vec<MarketResponse> {`

        `self.markets.values().map(|m| MarketResponse {`

            `market_id: m.market_id.clone(),`

            `underlying_asset: m.underlying_asset.clone(),`

            `settlement_currency: m.settlement_currency.clone(),`

            `market_type: m.market_type.clone(),`

            `status: m.status.clone(),`

        `}).collect()`

    `}`

    `pub fn get_market_instruments(&self, market_id: &str) -> Vec<InstrumentResponse> {`

        `self.instruments_by_market.get(market_id)`

            `.map(|instruments| instruments.iter().map(|i| InstrumentResponse {`

                `instrument_id: i.instrument_id.clone(),`

                `canonical_name: i.canonical_name.clone(),`

                `market_id: i.market_id.clone(),`

                `underlying_asset: i.underlying_asset.clone(),`

                `option_type: i.option_type.clone(),`

                `style: i.style.clone(),`

                `strike_price: i.strike_price,`

                `expiry_timestamp: i.expiry_timestamp,`

                `contract_size: i.contract_size,`

                `settlement_currency: i.settlement_currency.clone(),`

                `tick_size: i.tick_size,`

                `min_order_size: i.min_order_size,`

                `status: i.status.clone(),`

                `created_at: i.created_at,`

            `}).collect())`

            `.unwrap_or_default()`

    `}`

    `pub fn get_instrument_by_id(&self, instrument_id: &str) -> Option<InstrumentResponse> {`

        `self.instruments_by_id.get(instrument_id).map(|i| InstrumentResponse {`

            `instrument_id: i.instrument_id.clone(),`

            `canonical_name: i.canonical_name.clone(),`

            `market_id: i.market_id.clone(),`

            `underlying_asset: i.underlying_asset.clone(),`

            `option_type: i.option_type.clone(),`

            `style: i.style.clone(),`

            `strike_price: i.strike_price,`

            `expiry_timestamp: i.expiry_timestamp,`

            `contract_size: i.contract_size,`

            `settlement_currency: i.settlement_currency.clone(),`

            `tick_size: i.tick_size,`

            `min_order_size: i.min_order_size,`

            `status: i.status.clone(),`

            `created_at: i.created_at,`

        `})`

    `}`

`}`

---

## **4️⃣ Key Principles**

1. **Read-only:** APIs never mutate core state.

2. **Cached aggressively:** Avoid querying DB repeatedly for frequently requested instruments/markets.

3. **Consistency:** Data returned always matches **canonical state** in InstrumentRegistry.

4. **Separation of concerns:**

   * Registry stores state.

   * Read models expose it.

   * OMS, Matching, Risk Engine consume it.

---

## **5️⃣ Integration**

`Frontend / UI → GET /markets → show available markets`

`Frontend → GET /markets/{id}/instruments → list active options`

`OMS → GET /instruments/{id} → validate order against canonical metadata`

`Risk Engine → GET /instruments/{id} → calculate margin/PnL`

`Analytics → GET /markets → historical reporting`

* **Caching** is important because UI may poll frequently.

* Can use **in-memory cache** or **Redis** with TTL for high-throughput exchanges.

* **Deterministic responses** → essential for OMS and Risk Engine correctness.

# **LEVEL 0 — Primitives**

`// Base asset type`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct Asset {`

    `pub asset_id: String,    // BTC, ETH`

    `pub chain: String,       // Bitcoin, Ethereum`

    `pub decimals: u8,        // Precision`

`}`

`// Settlement currency type`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct Currency {`

    `pub currency_id: String, // USDT, USD`

    `pub decimals: u8,`

`}`

✅ Notes: Immutable facts about the world. No logic here.

---

# **2️⃣ LEVEL 1 — Market Definition**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum MarketType { Options }`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum MarketStatus { Active, Halted, Expired }`

`#[derive(Debug, Clone, Serialize, Deserialize)]`

`pub struct Market {`

    `pub market_id: String,               // e.g., BTC-OPTIONS-USDT`

    `pub underlying_asset: Asset,`

    `pub settlement_currency: Currency,`

    `pub market_type: MarketType,`

    `pub status: MarketStatus,`

`}`

* Market \= “tradeable universe”.

* Groups instruments, does not define them.

---

# **3️⃣ LEVEL 2 — Instrument Template**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionStyle { European }`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionType { Call, Put }`

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum ExerciseType { CashSettled } // v0 only`

* Defines the **shape of options**, not instances.

---

# **4️⃣ LEVEL 3 — Option Instrument**

`#[derive(Debug, Clone, Serialize, Deserialize)]`

`pub struct OptionInstrument {`

    `pub instrument_id: String,        // deterministic hash of canonical name`

    `pub canonical_name: String,       // human-readable`

    `pub market_id: String,`

    `pub underlying_asset: Asset,`

    `pub option_type: OptionType,`

    `pub style: OptionStyle,`

    `pub strike_price: u64,            // in settlement currency`

    `pub expiry_timestamp: u64,        // UTC seconds`

    `pub contract_size: u64,           // e.g., 1 BTC`

    `pub settlement_currency: Currency,`

    `pub tick_size: u64,`

    `pub min_order_size: u64,`

    `pub status: OptionStatus,`

    `pub created_at: u64,`

`}`

---

# **5️⃣ LEVEL 4 — Instrument Identity & Naming**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub struct OptionInstrumentId(String);`

`impl OptionInstrumentId {`

    `pub fn from_canonical_name(canonical_name: &str) -> Self {`

        `use sha2::{Sha256, Digest};`

        `let mut hasher = Sha256::new();`

        `hasher.update(canonical_name.as_bytes());`

        `Self(hex::encode(hasher.finalize()))`

    `}`

`}`

* Deterministic, immutable ID for all downstream references.

---

# **6️⃣ LEVEL 5 — Instrument Lifecycle**

`#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`

`pub enum OptionStatus {`

    `Draft,`

    `Listed,`

    `Active,`

    `Expired,`

    `Settled,`

    `Archived,`

`}`

`impl OptionStatus {`

    `pub fn can_transition_to(&self, next: &OptionStatus) -> bool {`

        `match self {`

            `OptionStatus::Draft => matches!(next, OptionStatus::Listed),`

            `OptionStatus::Listed => matches!(next, OptionStatus::Active),`

            `OptionStatus::Active => matches!(next, OptionStatus::Expired),`

            `OptionStatus::Expired => matches!(next, OptionStatus::Settled),`

            `OptionStatus::Settled => matches!(next, OptionStatus::Archived),`

            `OptionStatus::Archived => false,`

        `}`

    `}`

`}`

---

# **7️⃣ LEVEL 6 — Instrument Registry**

`use std::collections::HashMap;`

`#[derive(Debug, Default)]`

`pub struct InstrumentRegistry {`

    `pub instruments_by_market: HashMap<String, Vec<OptionInstrument>>,`

    `pub instruments_by_id: HashMap<String, OptionInstrument>,`

    `pub markets: HashMap<String, Market>,`

`}`

`impl InstrumentRegistry {`

    `// Admin-only methods`

    `pub fn add_market(&mut self, market: Market) { self.markets.insert(market.market_id.clone(), market); }`

    `pub fn add_instrument(&mut self, instrument: OptionInstrument) -> Result<(), String> {`

        `if !self.markets.contains_key(&instrument.market_id) {`

            `return Err("Market does not exist".to_string());`

        `}`

        `self.instruments_by_market.entry(instrument.market_id.clone())`

            `.or_insert_with(Vec::new)`

            `.push(instrument.clone());`

        `self.instruments_by_id.insert(instrument.instrument_id.clone(), instrument);`

        `Ok(())`

    `}`

    `// Read-only queries`

    `pub fn list_markets(&self) -> Vec<&Market> { self.markets.values().collect() }`

    `pub fn list_instruments(&self, market_id: &str) -> Vec<&OptionInstrument> {`

        `self.instruments_by_market.get(market_id).map(|v| v.iter().collect()).unwrap_or_default()`

    `}`

    `pub fn get_instrument(&self, instrument_id: &str) -> Option<&OptionInstrument> { self.instruments_by_id.get(instrument_id) }`

`}`

---

# **8️⃣ LEVEL 7 — Constraints & Validation**

`impl OptionInstrument {`

    `pub fn validate(&self, market_contract_size: u64) -> Result<(), String> {`

        `let now = chrono::Utc::now().timestamp() as u64;`

        `if self.expiry_timestamp <= now { return Err("Expiry must be in the future".into()); }`

        `if self.strike_price == 0 { return Err("Strike > 0".into()); }`

        `if self.strike_price % self.tick_size != 0 { return Err("Tick size divides strike cleanly".into()); }`

        `if self.contract_size != market_contract_size { return Err("Contract size fixed per market".into()); }`

        `Ok(())`

    `}`

`}`

---

# **9️⃣ LEVEL 8 — Time & Calendar Model**

`#[derive(Debug, Clone)]`

`pub struct TradingCalendar {`

    `pub timezone: chrono::FixedOffset, // UTC`

    `pub trading_days: Vec<chrono::Weekday>, // Crypto: all days`

`}`

`impl Default for TradingCalendar {`

    `fn default() -> Self {`

        `Self {`

            `timezone: chrono::FixedOffset::east(0),`

            `trading_days: vec![`

                `chrono::Weekday::Mon,`

                `chrono::Weekday::Tue,`

                `chrono::Weekday::Wed,`

                `chrono::Weekday::Thu,`

                `chrono::Weekday::Fri,`

                `chrono::Weekday::Sat,`

                `chrono::Weekday::Sun,`

            `],`

        `}`

    `}`

`}`

`impl OptionInstrument {`

    `pub fn has_expired(&self, current_ts: u64) -> bool { current_ts >= self.expiry_timestamp }`

`}`

---

# **10️⃣ LEVEL 9 — Read Models / APIs**

`#[derive(Debug, Serialize)]`

`pub struct MarketResponse { /* same fields as Market */ }`

`#[derive(Debug, Serialize)]`

`pub struct InstrumentResponse { /* same fields as OptionInstrument */ }`

`impl InstrumentRegistry {`

    `pub fn get_markets(&self) -> Vec<MarketResponse> { /* map markets → response */ }`

    `pub fn get_market_instruments(&self, market_id: &str) -> Vec<InstrumentResponse> { /* map instruments → response */ }`

    `pub fn get_instrument_by_id(&self, instrument_id: &str) -> Option<InstrumentResponse> { /* map instrument → response */ }`

`}`

##  **Asset & Currency Primitives**

* `Asset` \= BTC or ETH

  * `asset_id` \= "BTC" / "ETH"

  * `chain` \= which blockchain

  * `decimals` \= precision

* `Currency` \= USDT

  * Used for settlement only

  * `decimals` \= precision for pricing

✅ Correct: these are **immutable facts**, no logic, just the universe your options live in.

## **Market Definition**

* Market \= tradeable universe

* Each **underlying \+ settlement currency** pair gets its own market

* With your current setup:

  1. BTC-USDT market

  2. ETH-USDT market

✅ Correct: markets **group instruments**, they don’t define them.

##  **Instrument Templates**

* Define **shape of options** (before creating individual options)

* `OptionType` \= Call / Put

* `OptionStyle` \= European (no American-style exercises for now)

* `ExerciseType` \= CashSettled

* With 2 underlying assets × 2 option types → 4 “instrument groups”:

  1. BTC-USDT Call

  2. BTC-USDT Put

  3. ETH-USDT Call

  4. ETH-USDT Put

✅ Correct.

## **Core Option Instrument**

* Represents **each actual option someone creates**

* Fields:

  * `instrument_id` \= hash of canonical name (deterministic)

  * `market_id` \= market it belongs to

  * `underlying_asset`, `option_type`, `style`

  * `strike_price`

  * `expiry_timestamp` (UTC)

  * `contract_size` (how many BTC/ETH this option represents)

  * `settlement_currency`

  * `tick_size` (exchange-defined, e.g., 0.5 USDT)

  * `min_order_size` (minimum \# of contracts per order)

  * `status` \= Listed / Active / Expired / Settled

  * `created_at` \= UTC timestamp

✅ Correct.

### **Questions / Clarifications**

1. **Contract Size for big options (like 40 BTC)**

   * Options are usually **broken into 1 contract units**.

   * So 40 BTC → 40 individual contracts.

   * Buyers can buy **any number of contracts**, not the full lot.

   * This is **standard practice** and allows partial trading, bid/ask priority, and secondary market liquidity.

2. **Tick Size**

   * Yes, determined by exchange.

   * Ensures **prices align** to a standard increment.

   * Example: 0.5 USDT per option, so strikes and premiums snap to nearest multiple.

3. **Expiry & Exercise**

   * European-style options → can only be **exercised at expiry**.

   * They **cannot be exercised early**.

   * Once expiry timestamp is reached:

     * Option moves `ACTIVE → EXPIRED`

     * Then `EXPIRED → SETTLED`

   * After settlement, option is **removed from active listings** and stored permanently for historical records.

## **Instrument Registry**

* Responsibilities:

  * Create instruments

  * Validate invariants / constraints

  * Store **immutable metadata**

  * Expose **read-only queries** (list markets, list instruments, get instrument by ID)

* **Admin-only writes** → ensures consistency

## **Constraints & Validation**

* Expiry must be in the future

* Strike price \> 0

* Tick size divides strike cleanly

* Contract size fixed per market

✅ Correct: all instruments must pass this before being listed.

## **Trading Calendar**

* 24/7 crypto → all days trading

* UTC timestamps for everything

* Ensures **deterministic expiry / settlement**

## **Read Models / APIs**

* Expose markets / instruments to:

  * OMS (to validate orders)

  * Risk Engine (for margin / exposure)

  * Frontend / UI (show instruments & prices)

* All **read-only**, cached aggressively for performance

## **Contract Size vs Partial Filling**

* `contract_size` \= **exposure per contract**.

  * Example: `contract_size = 1 BTC` → 1 option contract represents 1 BTC of underlying.

* **Partial filling** happens at the **order level**, not the contract level.

### **What that means:**

* If someone places an **order for 10 contracts**, the OMS can partially fill it: e.g., 4 now, 6 later.

* But **each contract** itself is **atomic**: you cannot buy 0.3 of a single 1 BTC contract.

* So even if the total exposure is big (say 50 BTC → 50 contracts), the **OMS will split the order across multiple fills**, but **each contract stays intact**.

✅ So yes, **partial filling is possible**, but only at the **order aggregation level**, not breaking a single contract.

## **Minimum Order Size (`min_order_size`)**

* This is **the minimum number of contracts a user must buy in one order**.

* It’s **intrinsic to the system**, because:

  * You don’t want tiny orders that create unnecessary bookkeeping and clutter the order book.

  * Risk engine can manage positions more efficiently.

* Example:

  * `contract_size = 1 BTC`

  * `min_order_size = 5`  
     → User must buy **at least 5 contracts** (5 BTC exposure).  
     → Cannot place an order for 1, 2, 3, or 4 contracts.

**Fractional Contract Size**

* Example: `contract_size = 0.01 BTC`

* Each contract represents **0.01 BTC of underlying**.

* Makes retail participation feasible because the premium is now affordable.

**Min Order Size**

* Still enforces **minimum meaningful order**, usually 1 contract.

* Users can buy 1 contract (0.01 BTC exposure), 10 contracts (0.1 BTC), or more.

**Larger Exposure**

* Large traders just submit **more contracts in the same order**.

* OMS can fill these atomically, still respecting `min_order_size`.

**Partial Fills**

* The OMS can partially fill large orders across multiple buyers/sellers.

* **Each contract remains atomic**, just like before.

**Liquidity Benefits**

* Fractional contracts → more participants → tighter bid/ask spreads → better secondary market liquidity.

* Smaller lots are tradable, but large lots are still possible for institutional players.

| Contract Size | Min Order | Buyer A | Buyer B | Outcome |
| ----- | ----- | ----- | ----- | ----- |
| 0.01 BTC | 1 | 3 contracts | 7 contracts | Buyer A fills partially from Seller’s 10-contract order, Buyer B takes remaining 7 |
| 0.01 BTC | 1 | 100 contracts | \- | OMS splits across multiple sellers, each contract atomic |

*   
  **Atomic contracts \+ fractional exposure** \= flexible retail & institutional participation

* **Order book remains granular** but not cluttered with tiny uneconomical trades

Here’s a clean design:

`use chrono::{DateTime, Utc};`

`use serde::{Serialize, Deserialize};`

`use uuid::Uuid;`

`/// Underlying asset primitive`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub struct Asset {`

    `pub asset_id: String,   // "BTC" / "ETH"`

    `pub chain: String,      // "Bitcoin" / "Ethereum"`

    `pub decimals: u8,       // precision, e.g., 8`

`}`

`/// Settlement currency primitive`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub struct Currency {`

    `pub currency_id: String,  // "USDT"`

    `pub decimals: u8,         // precision`

`}`

`/// Market represents tradeable universe (Underlying + Settlement Currency)`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub struct Market {`

    `pub market_id: Uuid,`

    `pub underlying_asset: Asset,`

    `pub settlement_currency: Currency,`

    `pub market_type: MarketType,`

    `pub status: MarketStatus,`

`}`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub enum MarketType {`

    `Options,`

    `// Future: Futures, Swaps, etc.`

`}`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub enum MarketStatus {`

    `Active,`

    `Halted,`

    `Expired,`

`}`

`/// Option style and type`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub enum OptionStyle {`

    `European,`

    `// Future: American`

`}`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]`

`pub enum OptionType {`

    `Call,`

    `Put,`

`}`

`/// Core Option Instrument`

`#[derive(Debug, Clone, Serialize, Deserialize)]`

`pub struct OptionInstrument {`

    `pub instrument_id: String,       // deterministic hash of canonical name`

    `pub market_id: Uuid,             // market this instrument belongs to`

    `pub underlying_asset: Asset,`

    `pub option_type: OptionType,`

    `pub style: OptionStyle,`

    `pub strike_price: f64,           // in settlement currency`

    `pub expiry_timestamp: DateTime<Utc>,`

    `pub contract_size: f64,          // e.g., 0.01 BTC per contract`

    `pub min_order_size: u32,         // minimum number of contracts per order`

    `pub settlement_currency: Currency,`

    `pub tick_size: f64,              // price increments, e.g., 0.5 USDT`

    `pub status: InstrumentStatus,`

    `pub created_at: DateTime<Utc>,`

`}`

`#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]`

`pub enum InstrumentStatus {`

    `Draft,`

    `Listed,`

    `Active,`

    `Expired,`

    `Settled,`

    `Archived,`

`}`

`/// Canonical Instrument Name generator`

`impl OptionInstrument {`

    `pub fn canonical_name(&self) -> String {`

        `// Example: BTC-28MAR2026-50000-C`

        `format!(`

            `"{}-{}-{}-{}",`

            `self.underlying_asset.asset_id,`

            `self.expiry_timestamp.format("%d%b%Y").to_string().to_uppercase(),`

            `self.strike_price,`

            `match self.option_type {`

                `OptionType::Call => "C",`

                `OptionType::Put => "P",`

            `}`

        `)`

    `}`

    `pub fn instrument_id(&self) -> String {`

        `use sha2::{Sha256, Digest};`

        `let canonical = self.canonical_name();`

        `let mut hasher = Sha256::new();`

        `hasher.update(canonical.as_bytes());`

        `format!("{:x}", hasher.finalize())`

    `}`

`}`

---

### **✅ Notes on this design**

1. **Fractional contract size**

   * `contract_size: f64` → can be 0.01 BTC, 0.001 ETH, etc.

   * Makes retail trading feasible.

2. **Minimum order size**

   * `min_order_size: u32` → enforces how many contracts must be bought in a single order.

3. **Atomic contracts**

   * Each contract \= `contract_size` of underlying.

   * OMS can partially fill orders, but **each contract is indivisible**.

4. **Immutable primitives**

   * `Asset` and `Currency` are pure, immutable facts.

5. **Instrument lifecycle**

   * `status: InstrumentStatus` → moves `Draft → Listed → Active → Expired → Settled → Archived`.

6. **Instrument ID**

   * Deterministic hash of canonical name for **fast reads/writes** and **uniqueness**.

7. **Tick size**

   * Ensures **strike prices and premiums align** with exchange-defined increments.

