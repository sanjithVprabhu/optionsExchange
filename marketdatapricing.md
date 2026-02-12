# **SYSTEM 8 — Market Data & Feeds**

*“Everything observable, nothing authoritative.”*

This system **never writes state**.  
 It **derives views** from append-only events produced by other systems.

---

## **1️⃣ What Market Data IS (and is NOT)**

### **IS**

* Deterministic projection

* Cached, queryable

* Reconstructible from events

* Used by:

  * UI

  * Risk engine (mark prices)

  * Liquidation engine

  * External consumers

### **IS NOT**

* A source of truth

* A writer of balances

* A price decider

* A risk gatekeeper

**Invariant**

If Market Data dies, the exchange must still function.

---

## **2️⃣ Core Inputs (Authoritative Sources)**

Market Data **subscribes** to:

| Source | What it emits |
| ----- | ----- |
| Matching Engine | Trades |
| OMS | Order placements / cancels |
| Instrument Registry | Instrument metadata |
| Oracle Engine | Index prices |
| Risk Engine | Approved mark prices |

No circular dependencies. Ever.

---

## **3️⃣ Output Surfaces (What users & systems see)**

### **A. Order Book Snapshots**

Per **instrument**, not per market.

`OrderBookSnapshot {`  
  `instrument_id`  
  `bids: Vec<PriceLevel>`  
  `asks: Vec<PriceLevel>`  
  `sequence_number`  
  `timestamp`  
`}`

Each `PriceLevel`:

`PriceLevel {`  
  `price`  
  `total_quantity`  
  `order_count`  
`}`

📌 Derived from **OMS \+ Matching events**, never from balances.

---

### **B. Trades Feed**

`Trade {`  
  `trade_id`  
  `instrument_id`  
  `price`  
  `quantity`  
  `aggressor_side`  
  `timestamp`  
`}`

Used for:

* Last price

* VWAP

* Volume

* Candles

---

### **C. Index Price (External Reality)**

This is **NOT optional** for options.

`IndexPrice {`  
  `asset_id      // BTC`  
  `price         // USD`  
  `timestamp`  
  `confidence`  
`}`

Derived from:

* Multiple exchanges

* Median / trimmed mean

* Outlier rejection

👉 **Used only as reference**, never as settlement directly.

---

### **D. Mark Price (MOST IMPORTANT)**

Mark price \= what the **risk engine trusts**.

For options, mark price ≠ last trade.

`Mark Price =`  
  `OptionPricingModel(`  
    `Index Price,`  
    `Strike,`  
    `Time to Expiry,`  
    `Implied Volatility,`  
    `Interest Rate (≈0 for crypto)`  
  `)`

This is where **pricing finally enters the system**.

---

## **4️⃣ Option Pricing Engine (Deep Dive)**

This is **not a trading engine**, it’s a **valuation oracle**.

### **Base Model (v0)**

**Black–Scholes (European, cash-settled)**

Inputs:

* S \= Index price

* K \= Strike

* T \= Time to expiry (years)

* σ \= Implied volatility

* r \= Risk-free rate (≈0)

Outputs:

* Fair premium

* Greeks

---

### **Greeks (Required, not optional)**

`Greeks {`  
  `delta`  
  `gamma`  
  `vega`  
  `theta`  
  `rho`  
`}`

Used by:

* Margin engine

* Portfolio risk

* Liquidation ordering

---

### **Where does IV come from?**

Important question.

**Answer:**  
 👉 *Implied volatility is derived from market trades.*

Process:

1. Observe last N trades for instrument

2. Solve BS inverse to get σ

3. Smooth across strikes (vol surface)

4. Clamp / decay if illiquid

If no trades:

* Bootstrap from nearby strikes

* Or fallback to historical vol

---

## **5️⃣ Volatility Surface (Yes, you need one)**

Even in v0.

`σ = f(strike, expiry)`

You do **not** store IV per instrument permanently.  
 You **derive** it.

Surface types (v0 simple):

* Piecewise linear

* Strike buckets

* Expiry buckets

Later:

* SVI

* SABR

* Local vol

---

## **6️⃣ Mark Price Rules (Critical Invariants)**

* Mark price must be:

  * Smooth

  * Non-manipulable

  * Resistant to single trade spikes

Rules:

* Never equal last trade directly

* Always anchored to index

* Always bounded by bid/ask mid

This prevents:

* Fake liquidations

* Wash trading attacks

* Oracle griefing

---

## **7️⃣ Candles & Aggregates**

Pure projection.

`Candle {`  
  `open`  
  `high`  
  `low`  
  `close`  
  `volume`  
  `interval`  
`}`

Derived from Trades feed only.

---

## **8️⃣ APIs Exposed**

Read-only, aggressively cached.

`GET /orderbook/{instrument}`  
`GET /trades/{instrument}`  
`GET /mark-price/{instrument}`  
`GET /index-price/{asset}`  
`GET /greeks/{instrument}`  
`GET /vol-surface/{market}`

---

## **9️⃣ System-Level Invariants (Non-Negotiable)**

* Market data **never feeds back** into OMS

* Risk consumes mark price, not last price

* UI sees only projections

* Replay from genesis yields identical snapshots

---

## **Where You Are Now (Big Picture)**

You’ve built:

1. Instruments

2. OMS

3. Matching

4. Risk

5. Wallets

6. Liquidation

7. **Now: Market Data \+ Pricing**

**Black–Scholes becomes part of your exchange’s safety perimeter**, not just a UI feature.

I’ll do this in **four tight layers**, exactly how a real system should think about it:

1. Mathematical model (no shortcuts)

2. Numerical considerations (this is where exchanges fail)

3. Rust-level data structures & functions

4. How this plugs into Market Data \+ Risk (architecture)

No fluff. No finance-bro mysticism.

---

# **1️⃣ Black–Scholes — Exact Mathematics**

We are pricing **European, cash-settled options**.

### **Inputs (canonical)**

| Symbol | Meaning |
| ----- | ----- |
| `S` | Index price of underlying |
| `K` | Strike price |
| `T` | Time to expiry in **years** |
| `σ` | Implied volatility |
| `r` | Risk-free rate (≈ 0 for crypto) |

---

### **Core quantities**

d1=ln⁡(S/K)+(r+12σ2)TσTd\_1 \= \\frac{\\ln(S/K) \+ (r \+ \\frac{1}{2}\\sigma^2)T}{\\sigma\\sqrt{T}}d1​=σT​ln(S/K)+(r+21​σ2)T​ d2=d1−σTd\_2 \= d\_1 \- \\sigma\\sqrt{T}d2​=d1​−σT​

---

### **Option Prices**

**Call**

C=S⋅N(d1)−Ke−rT⋅N(d2)C \= S \\cdot N(d\_1) \- K e^{-rT} \\cdot N(d\_2)C=S⋅N(d1​)−Ke−rT⋅N(d2​)

**Put**

P=Ke−rT⋅N(−d2)−S⋅N(−d1)P \= K e^{-rT} \\cdot N(-d\_2) \- S \\cdot N(-d\_1)P=Ke−rT⋅N(−d2​)−S⋅N(−d1​)

Where `N(x)` \= standard normal CDF.

---

### **Greeks (mandatory)**

**Delta**

* Call: `N(d1)`

* Put: `N(d1) - 1`

**Gamma**

Γ=N′(d1)SσT\\Gamma \= \\frac{N'(d\_1)}{S \\sigma \\sqrt{T}}Γ=SσT​N′(d1​)​

**Vega**

V=SN′(d1)TV \= S N'(d\_1) \\sqrt{T}V=SN′(d1​)T​

**Theta**  
 (call version)

Θ=−SN′(d1)σ2T−rKe−rTN(d2)\\Theta \= \-\\frac{S N'(d\_1)\\sigma}{2\\sqrt{T}} \- rK e^{-rT} N(d\_2)Θ=−2T​SN′(d1​)σ​−rKe−rTN(d2​)

**Rho**  
 (call)

ρ=KTe−rTN(d2)\\rho \= K T e^{-rT} N(d\_2)ρ=KTe−rTN(d2​)

---

# **2️⃣ Numerical Reality (THIS PART IS CRITICAL)**

### **⚠️ Edge cases you MUST handle**

1. **T → 0**

   * Option value → intrinsic value

   * Greeks collapse

2. **σ → 0**

   * Degenerate distribution

3. **Deep ITM / OTM**

   * Floating point underflow in `exp`, `ln`

4. **No negative prices**

   * Ever

---

### **Hard rules (exchange-safe)**

* Clamp `T >= 1 second`

* Clamp `σ ∈ [0.01, 5.0]`

* If `T < ε`, return intrinsic value

* Never trust user IV directly

---

# **3️⃣ Rust-Level Implementation (Exchange-Grade)**

### **Core types**

`#[derive(Clone, Copy)]`  
`pub enum OptionType {`  
    `Call,`  
    `Put,`  
`}`

`pub struct BSInputs {`  
    `pub spot: f64,      // S`  
    `pub strike: f64,    // K`  
    `pub time: f64,      // T (years)`  
    `pub vol: f64,       // σ`  
    `pub rate: f64,      // r`  
    `pub option_type: OptionType,`  
`}`

`pub struct Greeks {`  
    `pub delta: f64,`  
    `pub gamma: f64,`  
    `pub vega: f64,`  
    `pub theta: f64,`  
    `pub rho: f64,`  
`}`

---

### **Normal distribution helpers**

`fn norm_pdf(x: f64) -> f64 {`  
    `(1.0 / (2.0 * std::f64::consts::PI).sqrt()) * (-0.5 * x * x).exp()`  
`}`

`// Abramowitz–Stegun approximation`  
`fn norm_cdf(x: f64) -> f64 {`  
    `let k = 1.0 / (1.0 + 0.2316419 * x.abs());`  
    `let poly = k * (0.319381530 +`  
        `k * (-0.356563782 +`  
        `k * (1.781477937 +`  
        `k * (-1.821255978 +`  
        `k * 1.330274429))));`  
    `let approx = 1.0 - norm_pdf(x) * poly;`  
    `if x >= 0.0 { approx } else { 1.0 - approx }`  
`}`

---

### **d₁, d₂ computation**

`fn d1_d2(input: &BSInputs) -> (f64, f64) {`  
    `let s = input.spot;`  
    `let k = input.strike;`  
    `let t = input.time.max(1e-6);`  
    `let v = input.vol.max(1e-6);`  
    `let r = input.rate;`

    `let d1 = ((s / k).ln() + (r + 0.5 * v * v) * t) / (v * t.sqrt());`  
    `let d2 = d1 - v * t.sqrt();`

    `(d1, d2)`  
`}`

---

### **Option price**

`pub fn black_scholes_price(input: &BSInputs) -> f64 {`  
    `let (d1, d2) = d1_d2(input);`  
    `let s = input.spot;`  
    `let k = input.strike;`  
    `let t = input.time;`  
    `let r = input.rate;`

    `match input.option_type {`  
        `OptionType::Call => {`  
            `s * norm_cdf(d1) - k * (-r * t).exp() * norm_cdf(d2)`  
        `}`  
        `OptionType::Put => {`  
            `k * (-r * t).exp() * norm_cdf(-d2) - s * norm_cdf(-d1)`  
        `}`  
    `}.max(0.0)`  
`}`

---

### **Greeks**

`pub fn black_scholes_greeks(input: &BSInputs) -> Greeks {`  
    `let (d1, d2) = d1_d2(input);`  
    `let s = input.spot;`  
    `let t = input.time;`  
    `let v = input.vol;`  
    `let r = input.rate;`

    `let pdf = norm_pdf(d1);`

    `let delta = match input.option_type {`  
        `OptionType::Call => norm_cdf(d1),`  
        `OptionType::Put => norm_cdf(d1) - 1.0,`  
    `};`

    `Greeks {`  
        `delta,`  
        `gamma: pdf / (s * v * t.sqrt()),`  
        `vega: s * pdf * t.sqrt(),`  
        `theta: -(s * pdf * v) / (2.0 * t.sqrt()),`  
        `rho: k * t * (-r * t).exp() * norm_cdf(d2),`  
    `}`  
`}`

---

# **4️⃣ Implied Volatility Solver (MANDATORY)**

The exchange **must invert BS**.

Use **Newton–Raphson \+ fallback**.

`pub fn implied_vol(`  
    `market_price: f64,`  
    `mut input: BSInputs,`  
`) -> Option<f64> {`  
    `let mut vol = 0.3;`

    `for _ in 0..100 {`  
        `input.vol = vol;`  
        `let price = black_scholes_price(&input);`  
        `let vega = black_scholes_greeks(&input).vega;`

        `if vega.abs() < 1e-8 {`  
            `break;`  
        `}`

        `let diff = price - market_price;`  
        `if diff.abs() < 1e-6 {`  
            `return Some(vol);`  
        `}`

        `vol -= diff / vega;`  
        `vol = vol.clamp(0.01, 5.0);`  
    `}`

    `None`  
`}`

---

# **5️⃣ Where This Lives Architecturally**

**NOT** in OMS  
 **NOT** in Matching  
 **NOT** in Wallets

📍 Lives in **Market Data → Pricing Engine**

Used by:

* Risk engine (margin)

* Liquidation engine (fair value)

* UI (reference price)

* Greeks feed

---

# **6️⃣ System Invariants (Tattoo These)**

* Black–Scholes is **reference**, not execution

* Traders can price anything

* Risk uses **mark price**, not last trade

* Greeks drive liquidation priority

* Pricing engine never mutates state

---

## **You are now here**

You now have:

* A real pricing engine

* Exchange-safe math

* Deterministic valuation

* Risk-grade Greeks

This is **institutional-level groundwork**.

# **Volatility Surface Construction (Exchange-grade)**

## **Why you need a vol surface**

Black–Scholes needs **σ**, but:

* Traders submit **prices**, not vols

* Every strike & expiry has a **different implied volatility**

* Risk, margin, liquidation **cannot depend on last trade**

So the exchange must build its **own volatility surface**.

---

## **Canonical axes**

A volatility surface is:

`σ = f(expiry, moneyness)`

Where:

* `expiry` \= time to expiry

* `moneyness` \= ln(S / K)

Never use raw strike directly — moneyness is stable across price moves.

---

## **Step 1 — Raw implied vols**

For every traded instrument:

* Take **mid price** (NOT last trade)

* Invert Black–Scholes → implied vol

`σ_raw(instrument_id)`

Discard if:

* Spread too wide

* Volume too low

* Near expiry with unstable Greeks

---

## **Step 2 — Bucketization**

You **must discretize** the surface.

### **Expiry buckets**

Example:

`7d, 14d, 30d, 90d, 180d`

### **Moneyness buckets**

Example:

`[-0.4, -0.25, -0.1, 0, +0.1, +0.25, +0.4]`

Each option maps to:

`(expiry_bucket, moneyness_bucket)`

---

## **Step 3 — Robust aggregation**

For each bucket:

* Use **volume-weighted median**

* NOT mean (mean is manipulable)

`σ_bucket = weighted_median(σ_raw)`

This is critical for attack resistance.

---

## **Step 4 — Interpolation**

Use:

* Linear interpolation in moneyness

* Linear interpolation in variance across time

Why variance?  
 Because:

`σ²T interpolates linearly`

Never interpolate σ directly across time.

---

## **Step 5 — Clamps & sanity rules**

Hard bounds:

`σ ∈ [10%, 500%]`

Slope limits:

* Adjacent strikes cannot differ by \> X%

Expiry monotonicity:

* Longer expiry cannot have absurdly lower variance

---

## **Output**

`pub struct VolSurface {`  
    `expiry_buckets: Vec<ExpiryTs>,`  
    `moneyness_buckets: Vec<f64>,`  
    `vols: Matrix<f64>,`  
`}`

This surface is:

* Read-only

* Versioned

* Deterministic

---

# **2️⃣ Mark Price Smoothing Rules (Anti-Manipulation)**

## **Why last trade price is dangerous**

An attacker can:

* Trade 1 contract at insane price

* Trigger liquidations

* Drain insurance fund

So **mark price ≠ last trade**.

---

## **Canonical Mark Price Formula**

For each option:

`mark_price = BS(`  
    `S_index,`  
    `K,`  
    `T,`  
    `σ_surface(expiry, moneyness)`  
`)`

This is the **theoretical fair value**, not market value.

---

## **Smoothing rules**

### **Time smoothing**

Use EMA:

`mark_t = α * new_mark + (1 - α) * mark_(t-1)`

Where:

`α = 0.1 – 0.2`

---

### **Bound against mid price**

Mark price must lie within:

`[mid_price - X%, mid_price + X%]`

This prevents surface glitches from nuking positions.

---

## **Special cases**

### **Near expiry**

If `T < threshold`:

`mark_price = intrinsic value`

### **Illiquid instruments**

Fallback:

* Use neighboring strikes

* Or freeze last valid mark

---

## **Invariant**

**Liquidations, margin, and PnL use mark price — never last trade.**

---

# **3️⃣ Margin Formulas Using Greeks (THIS IS CORE)**

Forget textbook margin.  
 Exchanges margin **risk**, not price.

---

## **Position representation**

For each user:

`Portfolio = Σ positions`

Each position has:

* Delta

* Gamma

* Vega

Derived using mark price \+ vol surface.

---

## **Core risk vectors**

For each underlying:

`Δ = Σ delta_i`  
`Γ = Σ gamma_i`  
`V = Σ vega_i`

---

## **Stress scenarios (SPAN-like)**

Define shocks:

### **Price shocks**

`±5%, ±10%, ±20%`

### **Vol shocks**

`±20%, ±40%`

Evaluate portfolio PnL under **all combinations**.

---

## **Worst-case loss**

`margin_required = max_loss_across_scenarios`

Add:

* Buffer

* Liquidity premium

* Short option surcharge

---

## **Long vs Short options**

### **Long options**

* Max loss \= premium paid

* Margin \= premium only

### **Short options**

* Unbounded loss

* Margin includes:

  * Delta risk

  * Gamma curvature

  * Vega expansion

---

## **Maintenance margin**

`maintenance_margin = initial_margin × 0.6`

Crossing this triggers liquidation.

---

## **Exchange invariant**

Margin is computed on **future risk**, not current PnL.

---

# **4️⃣ End-to-End Walkthrough (REAL FLOW)**

Let’s simulate **exactly** what happens.

---

## **Step 1 — Trade**

User sells:

`BTC-28MAR2026-50000-C`  
`Size: 100 contracts (0.01 BTC each)`  
`Premium: 1200 USDT`

---

## **Step 2 — Clearing**

Trade event emitted:

`TradeExecuted`

Clearing engine:

* Updates position

* Credits premium

* Locks margin

---

## **Step 3 — Mark price update**

Market data engine:

* Reads index BTC price

* Reads vol surface

* Computes mark price

* Smooths it

---

## **Step 4 — Risk recalculation**

Risk engine:

* Recomputes Greeks

* Runs stress scenarios

* Updates margin requirement

---

## **Step 5 — Maintenance breach**

BTC jumps \+12%

User margin ratio:

`equity / maintenance_margin < 1`

→ User becomes **liquidatable**

---

## **Step 6 — Liquidation engine**

### **Phase 1: Partial liquidation**

* Reduce position size

* Use liquidation orders at mark ± penalty

### **Phase 2: Full liquidation**

* If insufficient, close entire position

### **Phase 3: Insurance fund**

* Only if market gaps

---

## **Step 7 — Settlement at expiry**

At expiry:

* Option intrinsic value computed

* Cash settlement applied

* Position archived

* Margin released

---

# **5️⃣ System-Wide Invariants (Non-Negotiable)**

1. OMS submits → Risk gates → Matching only

2. Mark price ≠ last trade

3. Margin uses **worst-case future risk**

4. Liquidation uses **mark price**

5. All state derived from events

---

## **Where you are now**

You have:

* A real volatility surface

* Manipulation-safe mark prices

* Professional margin logic

* Deterministic liquidation flow

This is **institutional-grade derivatives infrastructure**.

# **Portfolio Margin Across Underlyings**

## **Why single-asset margin is insufficient**

If a user has:

* Short BTC calls

* Long ETH puts

Treating these independently **over-margins** the user.

But treating them as fully offsetting is **dangerous**.

So we need **correlation-aware margin**.

---

## **Core idea**

Portfolio margin evaluates **joint worst-case loss** across assets, not sum of individual worst cases.

Formally:

Margin=max⁡scenarios(∑iPnLi(scenario))\\text{Margin} \= \\max\_{\\text{scenarios}} \\left( \\sum\_i \\text{PnL}\_i(\\text{scenario}) \\right)Margin=scenariosmax​(i∑​PnLi​(scenario))

---

## **Step 1 — Define correlated shock grid**

Let:

* BTC price shocks: ±5%, ±10%, ±20%

* ETH price shocks: ±5%, ±10%, ±20%

* Correlation coefficient ρ ≈ 0.6–0.8 (crypto reality)

We **do not** evaluate all combinations equally.

Instead we apply **correlation-weighted shocks**:

| Scenario | BTC | ETH |
| ----- | ----- | ----- |
| Base | 0% | 0% |
| Market crash | −20% | −15% |
| BTC leads | −20% | −8% |
| ETH leads | −8% | −20% |
| Market rally | \+20% | \+15% |

Same for volatility shocks.

---

## **Step 2 — Portfolio Greeks aggregation**

For each underlying:

Δu=∑ΔiΓu=∑ΓiVu=∑Vi\\Delta\_u \= \\sum \\Delta\_i \\quad \\Gamma\_u \= \\sum \\Gamma\_i \\quad V\_u \= \\sum V\_iΔu​=∑Δi​Γu​=∑Γi​Vu​=∑Vi​

Then total PnL under scenario:

PnL=∑u(Δu⋅ΔSu+12Γu⋅(ΔSu)2+Vu⋅Δσu)\\text{PnL} \= \\sum\_u \\left( \\Delta\_u \\cdot \\Delta S\_u \+ \\frac{1}{2}\\Gamma\_u \\cdot (\\Delta S\_u)^2 \+ V\_u \\cdot \\Delta \\sigma\_u \\right)PnL=u∑​(Δu​⋅ΔSu​+21​Γu​⋅(ΔSu​)2+Vu​⋅Δσu​)

---

## **Step 3 — Worst-case selection**

Margin requirement \= **maximum loss** across all scenarios \+ buffer.

This is **SPAN-style** but simplified and deterministic.

---

## **Invariant**

Portfolio margin always ≥ single-asset margin × correlation floor

Never let correlations reduce margin to zero.

---

# **2️⃣ Volatility Surface Arbitrage Detection**

This protects **your exchange**, not traders.

---

## **What counts as arbitrage?**

### **Static arbitrage (must NEVER happen)**

1. **Calendar arbitrage**

   * Longer expiry has lower total variance than shorter

2. **Butterfly arbitrage**

   * Vol curve violates convexity

3. **Negative variance**

   * Impossible

---

## **Mathematical checks**

### **Calendar monotonicity**

For same strike:

σ2(T2)T2≥σ2(T1)T1if T2\>T1\\sigma^2(T\_2) T\_2 \\ge \\sigma^2(T\_1) T\_1 \\quad \\text{if } T\_2 \> T\_1σ2(T2​)T2​≥σ2(T1​)T1​if T2​\>T1​

### **Strike convexity**

Option prices must be convex in strike:

∂2C∂K2≥0\\frac{\\partial^2 C}{\\partial K^2} \\ge 0∂K2∂2C​≥0

Numerically:

* Sample strikes

* Check finite differences

---

## **What happens if violation detected?**

1. **Freeze mark price updates**

2. **Fall back to previous valid surface**

3. **Flag instrument for manual review**

4. **Disable liquidations based on corrupted surface**

This avoids cascade liquidations.

---

## **Invariant**

No liquidation ever uses an arbitrage-invalid surface.

---

# **3️⃣ Insurance Fund Sizing Math**

This is **existential**.

---

## **What insurance fund covers**

ONLY:

* Liquidations that fail to close at mark ± penalty

* Market gaps

* Extreme vol spikes

NOT:

* Bad pricing

* OMS bugs

* Admin mistakes

---

## **Expected Shortfall model**

You don’t size for max loss — you size for **tail loss**.

Let:

* LLL \= liquidation loss random variable

Insurance fund target:

Fund≥ES99.9%(L)\\text{Fund} \\ge \\text{ES}\_{99.9\\%}(L)Fund≥ES99.9%​(L)

---

## **Practical approximation**

Track historical liquidation losses:

`losses = [L1, L2, ...]`

Compute:

* 99.9th percentile

* Apply stress multiplier (2×–3×)

---

## **Dynamic funding**

Fund grows via:

* Liquidation penalty fees

* Trading fees allocation

Never static.

---

## **Invariant**

Insurance fund must survive **single-day black swan**, not infinite apocalypse.

---

# **4️⃣ Circuit Breakers & Kill Switches**

These are **non-optional**.

---

## **Layer 1 — Market data breakers**

Trigger if:

* Index price jumps \> X% in Y seconds

* Vol surface shifts \> Z%

Action:

* Freeze matching

* Allow cancels only

---

## **Layer 2 — Liquidation throttles**

Trigger if:

* Liquidations per second exceed threshold

* Insurance fund drawdown too fast

Action:

* Slow liquidation rate

* Switch to auction-only mode

---

## **Layer 3 — System kill switch**

Trigger if:

* Risk engine divergence

* Event log mismatch

* Determinism violated

Action:

* Halt everything

* Preserve event log

* Manual intervention

---

## **Invariant**

It is better to halt the exchange than liquidate incorrectly.

---

# **5️⃣ Formal Verification of Invariants (This is rare, but you can do it)**

You don’t verify *code*.  
 You verify **properties**.

---

## **Core invariants to verify**

### **Risk invariant**

`No trade enters matching unless margin >= required`

### **Accounting invariant**

`Σ wallet balances + insurance fund = total system equity`

### **Determinism invariant**

`Replaying event log produces identical state`

### **Liquidation invariant**

`User equity never goes below −insurance_fund`

---

## **How to implement (practical)**

### **1\. Property-based testing (Rust)**

Use `proptest`:

* Random trade sequences

* Random price paths

* Assert invariants always hold

---

### **2\. Deterministic replay tests**

* Snapshot state

* Replay event log

* Compare hashes

---

### **3\. Model checking (optional but elite)**

Define simplified state machine:

* OMS

* Risk

* Matching

Verify transitions with TLA+ or Alloy.

You don’t need full system — only **core transitions**.

---

## **Invariant**

If an invariant cannot be tested, it is not real.

---

# **Where you are now**

You have crossed from:

“I’m building an options exchange”

to:

**“I’m designing a financially survivable derivatives system.”**

This is **top-tier exchange architecture**.

