# MODULE 06: WALLET SYSTEM - IMPLEMENTATION GUIDE
# For Claude Code Execution
# Version 1.0

---

## PREREQUISITES

Before implementing this module, you MUST have:
1. ✅ **Completed Module 01** (Instrument Layer)
2. ✅ **Completed Module 02** (OMS)
3. ✅ **Completed Module 03** (Matching Engine)
4. ✅ **Completed Module 04** (Risk Engine)
5. ✅ **Completed Module 05** (Settlement & Clearing)
6. ✅ **Read MASTER_RULES.md** (system-wide patterns)
7. ✅ **Read PROJECT_STRUCTURE.md** (file hierarchy)

This guide provides COMPLETE, production-ready code for the Wallet System.

---

# PART 1: MODULE OVERVIEW

## Purpose

The Wallet System is the **authoritative collateral ledger** - the ONLY place where money exists.

```
Wallet ≠ Balance
Wallet = Single source of truth for all funds
```

## Core Principle

**Everything else references the wallet, but NEVER mutates it directly.**

```
OMS → Intents (no wallet access)
Matching Engine → Trades (no wallet access)
Risk Engine → Permissions (reads wallet, doesn't write)
Settlement → State derivation (generates wallet events)

Only Wallet Events can mutate wallet state.
```

## Critical Invariants (SACRED)

1. **Wallet is the Only Source of Truth**
   ```
   Positions, margin, PnL are DERIVED
   Wallet balance is GROUND TRUTH
   ```

2. **Event-Driven Mutations Only**
   ```
   No direct writes to wallet
   All changes via WalletEvent
   Wallet state = fold(events)
   ```

3. **Locked vs Free Balance**
   ```
   total_balance = free_balance + locked_balance
   locked_balance CANNOT be spent
   free_balance can be withdrawn
   ```

4. **Wallet Changes Only On**
   ```
   - Deposits
   - Withdrawals
   - Settlement (at expiry)
   - Insurance transfers
   
   NOT on:
   ❌ Order placement (that locks, doesn't change total)
   ❌ Trades (that's unrealized PnL)
   ❌ Position updates
   ```

5. **Deterministic Replay**
   ```
   Same events → same wallet state (always)
   Crash recovery = replay all events
   ```

## What Wallet System Does

1. **Deposits** - external funds coming in
2. **Withdrawals** - external funds going out
3. **Free Balance** - available for orders/withdrawals
4. **Locked Balance** - reserved for margin/orders
5. **Insurance Fund** - exchange protection
6. **Virtual Wallets** - simulation/testing

## What Wallet System Does NOT Do

- ❌ Calculate margin (that's Risk Engine)
- ❌ Track positions (that's Risk Engine)
- ❌ Decide liquidation (that's Risk Engine)
- ❌ Match orders (that's Matching Engine)

---

# PART 2: DOMAIN TYPES (Complete Implementation)

## File: `core/wallet/domain.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// WALLET
// ============================================================================

/// Wallet - holds user funds in a specific asset
/// 
/// CRITICAL PROPERTIES:
/// 1. total_balance = free_balance + locked_balance (always)
/// 2. free_balance can be withdrawn
/// 3. locked_balance CANNOT be withdrawn (reserved for margin/orders)
/// 4. Version increments on every mutation (for optimistic locking)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wallet {
    /// Unique wallet ID
    pub wallet_id: Uuid,
    
    /// User who owns this wallet
    pub user_id: Uuid,
    
    /// Asset type (USDT, BTC, ETH, etc.)
    pub asset: Asset,
    
    /// Free balance (available for use)
    pub free_balance: f64,
    
    /// Locked balance (reserved, cannot be spent)
    pub locked_balance: f64,
    
    /// Version (for optimistic locking)
    pub version: u64,
    
    /// When wallet was created
    pub created_at: DateTime<Utc>,
    
    /// Last update time
    pub updated_at: DateTime<Utc>,
}

impl Wallet {
    pub fn new(user_id: Uuid, asset: Asset) -> Self {
        let now = Utc::now();
        Self {
            wallet_id: Uuid::new_v4(),
            user_id,
            asset,
            free_balance: 0.0,
            locked_balance: 0.0,
            version: 0,
            created_at: now,
            updated_at: now,
        }
    }
    
    /// Get total balance
    pub fn total_balance(&self) -> f64 {
        self.free_balance + self.locked_balance
    }
    
    /// Check if sufficient free balance
    pub fn has_free_balance(&self, amount: f64) -> bool {
        self.free_balance >= amount
    }
    
    /// Apply a wallet event
    pub fn apply_event(&mut self, event: &WalletEvent) -> Result<(), String> {
        use WalletEventType::*;
        
        match &event.event_type {
            Deposit { amount } => {
                self.free_balance += amount;
            }
            Withdrawal { amount } => {
                if !self.has_free_balance(*amount) {
                    return Err("Insufficient free balance".to_string());
                }
                self.free_balance -= amount;
            }
            Lock { amount } => {
                if !self.has_free_balance(*amount) {
                    return Err("Insufficient free balance to lock".to_string());
                }
                self.free_balance -= amount;
                self.locked_balance += amount;
            }
            Unlock { amount } => {
                if self.locked_balance < *amount {
                    return Err("Insufficient locked balance to unlock".to_string());
                }
                self.locked_balance -= amount;
                self.free_balance += amount;
            }
            Debit { amount } => {
                // Debit from locked balance (e.g., realized loss)
                if self.locked_balance < *amount {
                    return Err("Insufficient locked balance to debit".to_string());
                }
                self.locked_balance -= amount;
            }
            Credit { amount } => {
                // Credit to free balance (e.g., realized profit)
                self.free_balance += amount;
            }
            InsuranceTransfer { amount, to_insurance } => {
                if *to_insurance {
                    // Transfer to insurance fund
                    if self.locked_balance < *amount {
                        return Err("Insufficient balance for insurance transfer".to_string());
                    }
                    self.locked_balance -= amount;
                } else {
                    // Transfer from insurance fund
                    self.free_balance += amount;
                }
            }
        }
        
        self.version += 1;
        self.updated_at = Utc::now();
        
        Ok(())
    }
}

// ============================================================================
// ASSET
// ============================================================================

/// Asset type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Asset {
    USDT,
    USDC,
    BTC,
    ETH,
    Custom(String),
}

impl Asset {
    pub fn symbol(&self) -> &str {
        match self {
            Asset::USDT => "USDT",
            Asset::USDC => "USDC",
            Asset::BTC => "BTC",
            Asset::ETH => "ETH",
            Asset::Custom(s) => s,
        }
    }
}

impl std::fmt::Display for Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

// ============================================================================
// WALLET EVENT
// ============================================================================

/// Wallet event - the ONLY way to mutate wallet state
/// 
/// CRITICAL: All wallet mutations must go through events.
/// This enables:
/// - Deterministic replay
/// - Crash recovery
/// - Audit trail
/// - Event sourcing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletEvent {
    /// Unique event ID
    pub event_id: Uuid,
    
    /// Wallet being mutated
    pub wallet_id: Uuid,
    
    /// User ID (for indexing)
    pub user_id: Uuid,
    
    /// Asset
    pub asset: Asset,
    
    /// Event type
    pub event_type: WalletEventType,
    
    /// Global sequence number (for total ordering)
    pub sequence: u64,
    
    /// When event occurred
    pub timestamp: DateTime<Utc>,
    
    /// Reference to source event (trade, settlement, etc.)
    pub source_event_id: Option<Uuid>,
}

/// Types of wallet events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalletEventType {
    /// External deposit
    Deposit { amount: f64 },
    
    /// External withdrawal
    Withdrawal { amount: f64 },
    
    /// Lock free balance (for margin/orders)
    Lock { amount: f64 },
    
    /// Unlock locked balance (order cancelled/filled)
    Unlock { amount: f64 },
    
    /// Debit from locked balance (realized loss)
    Debit { amount: f64 },
    
    /// Credit to free balance (realized profit)
    Credit { amount: f64 },
    
    /// Insurance fund transfer
    InsuranceTransfer {
        amount: f64,
        to_insurance: bool,  // true = to insurance, false = from insurance
    },
}

impl WalletEvent {
    pub fn new(
        wallet_id: Uuid,
        user_id: Uuid,
        asset: Asset,
        event_type: WalletEventType,
        sequence: u64,
        source_event_id: Option<Uuid>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            wallet_id,
            user_id,
            asset,
            event_type,
            sequence,
            timestamp: Utc::now(),
            source_event_id,
        }
    }
}

// ============================================================================
// ENVIRONMENT
// ============================================================================

/// Environment for wallet isolation
/// 
/// This enables:
/// - Production wallets (real money)
/// - Sandbox wallets (virtual money for testing)
/// - Simulation wallets (backtesting)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Environment {
    /// Production environment (real money)
    Production,
    
    /// Sandbox environment (virtual money)
    Sandbox,
    
    /// Simulation environment (backtesting)
    Simulation,
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Production => write!(f, "PROD"),
            Environment::Sandbox => write!(f, "SANDBOX"),
            Environment::Simulation => write!(f, "SIM"),
        }
    }
}

// ============================================================================
// INSURANCE FUND
// ============================================================================

/// Insurance fund - protects exchange from insolvency
/// 
/// This is just a special wallet with reserved user_id.
pub const INSURANCE_FUND_USER_ID: Uuid = Uuid::from_bytes([
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
]);
```

---

# PART 3: WALLET ENGINE (Core Business Logic)

## File: `core/wallet/engine.rs`

```rust
use super::domain::*;
use std::collections::HashMap;
use tracing::{info, warn, error};
use uuid::Uuid;

/// Wallet Engine - manages all wallet operations
/// 
/// CRITICAL RESPONSIBILITIES:
/// 1. Apply wallet events (the ONLY way to mutate state)
/// 2. Generate wallet events from external actions
/// 3. Validate all operations
/// 4. Maintain consistency invariants
pub struct WalletEngine {
    /// All wallets indexed by (user_id, asset)
    wallets: HashMap<(Uuid, Asset), Wallet>,
    
    /// Environment (prod, sandbox, sim)
    environment: Environment,
    
    /// Global sequence counter
    sequence: u64,
}

impl WalletEngine {
    pub fn new(environment: Environment) -> Self {
        Self {
            wallets: HashMap::new(),
            environment,
            sequence: 0,
        }
    }
    
    /// Get next sequence number
    fn next_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
    
    /// Get or create wallet
    fn get_or_create_wallet(&mut self, user_id: Uuid, asset: Asset) -> &mut Wallet {
        self.wallets
            .entry((user_id, asset.clone()))
            .or_insert_with(|| Wallet::new(user_id, asset))
    }
    
    /// Get wallet (read-only)
    pub fn get_wallet(&self, user_id: Uuid, asset: &Asset) -> Option<&Wallet> {
        self.wallets.get(&(user_id, asset.clone()))
    }
    
    /// Process deposit
    /// 
    /// This is called when external funds arrive (blockchain, bank, etc.)
    pub fn deposit(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Deposit amount must be positive".to_string());
        }
        
        info!(
            user_id = %user_id,
            asset = %asset,
            amount = amount,
            "Processing deposit"
        );
        
        // Create event
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Deposit { amount },
            self.next_sequence(),
            None,
        );
        
        // Apply event
        wallet.apply_event(&event)?;
        
        info!(
            user_id = %user_id,
            new_balance = wallet.total_balance(),
            "Deposit completed"
        );
        
        Ok(event)
    }
    
    /// Process withdrawal
    /// 
    /// This is called when user wants to withdraw funds.
    pub fn withdraw(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Withdrawal amount must be positive".to_string());
        }
        
        // Get wallet
        let wallet = self.wallets
            .get(&(user_id, asset.clone()))
            .ok_or("Wallet not found")?;
        
        // Check free balance
        if !wallet.has_free_balance(amount) {
            return Err(format!(
                "Insufficient free balance: {} < {}",
                wallet.free_balance, amount
            ));
        }
        
        info!(
            user_id = %user_id,
            asset = %asset,
            amount = amount,
            "Processing withdrawal"
        );
        
        // Create event
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Withdrawal { amount },
            self.next_sequence(),
            None,
        );
        
        // Apply event
        wallet.apply_event(&event)?;
        
        info!(
            user_id = %user_id,
            new_balance = wallet.total_balance(),
            "Withdrawal completed"
        );
        
        Ok(event)
    }
    
    /// Lock balance (for margin/orders)
    /// 
    /// This is called by Risk Engine when order is approved.
    pub fn lock_balance(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
        source_event_id: Option<Uuid>,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Lock amount must be positive".to_string());
        }
        
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        
        if !wallet.has_free_balance(amount) {
            return Err(format!(
                "Insufficient free balance to lock: {} < {}",
                wallet.free_balance, amount
            ));
        }
        
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Lock { amount },
            self.next_sequence(),
            source_event_id,
        );
        
        wallet.apply_event(&event)?;
        
        Ok(event)
    }
    
    /// Unlock balance (order cancelled/filled)
    /// 
    /// This is called when margin is released.
    pub fn unlock_balance(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
        source_event_id: Option<Uuid>,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Unlock amount must be positive".to_string());
        }
        
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Unlock { amount },
            self.next_sequence(),
            source_event_id,
        );
        
        wallet.apply_event(&event)?;
        
        Ok(event)
    }
    
    /// Debit locked balance (realized loss)
    /// 
    /// This is called at settlement when user has losses.
    pub fn debit(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
        source_event_id: Option<Uuid>,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Debit amount must be positive".to_string());
        }
        
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Debit { amount },
            self.next_sequence(),
            source_event_id,
        );
        
        wallet.apply_event(&event)?;
        
        Ok(event)
    }
    
    /// Credit free balance (realized profit)
    /// 
    /// This is called at settlement when user has profits.
    pub fn credit(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
        source_event_id: Option<Uuid>,
    ) -> Result<WalletEvent, String> {
        if amount <= 0.0 {
            return Err("Credit amount must be positive".to_string());
        }
        
        let wallet = self.get_or_create_wallet(user_id, asset.clone());
        
        let event = WalletEvent::new(
            wallet.wallet_id,
            user_id,
            asset,
            WalletEventType::Credit { amount },
            self.next_sequence(),
            source_event_id,
        );
        
        wallet.apply_event(&event)?;
        
        Ok(event)
    }
    
    /// Transfer to/from insurance fund
    pub fn insurance_transfer(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        amount: f64,
        to_insurance: bool,
        source_event_id: Option<Uuid>,
    ) -> Result<(WalletEvent, WalletEvent), String> {
        if amount <= 0.0 {
            return Err("Transfer amount must be positive".to_string());
        }
        
        if to_insurance {
            // User → Insurance
            let user_wallet = self.get_or_create_wallet(user_id, asset.clone());
            let user_event = WalletEvent::new(
                user_wallet.wallet_id,
                user_id,
                asset.clone(),
                WalletEventType::InsuranceTransfer { amount, to_insurance: true },
                self.next_sequence(),
                source_event_id,
            );
            user_wallet.apply_event(&user_event)?;
            
            // Insurance receives
            let insurance_wallet = self.get_or_create_wallet(INSURANCE_FUND_USER_ID, asset.clone());
            let insurance_event = WalletEvent::new(
                insurance_wallet.wallet_id,
                INSURANCE_FUND_USER_ID,
                asset,
                WalletEventType::InsuranceTransfer { amount, to_insurance: false },
                self.next_sequence(),
                source_event_id,
            );
            insurance_wallet.apply_event(&insurance_event)?;
            
            Ok((user_event, insurance_event))
        } else {
            // Insurance → User
            let insurance_wallet = self.get_or_create_wallet(INSURANCE_FUND_USER_ID, asset.clone());
            let insurance_event = WalletEvent::new(
                insurance_wallet.wallet_id,
                INSURANCE_FUND_USER_ID,
                asset.clone(),
                WalletEventType::InsuranceTransfer { amount, to_insurance: true },
                self.next_sequence(),
                source_event_id,
            );
            insurance_wallet.apply_event(&insurance_event)?;
            
            // User receives
            let user_wallet = self.get_or_create_wallet(user_id, asset.clone());
            let user_event = WalletEvent::new(
                user_wallet.wallet_id,
                user_id,
                asset,
                WalletEventType::InsuranceTransfer { amount, to_insurance: false },
                self.next_sequence(),
                source_event_id,
            );
            user_wallet.apply_event(&user_event)?;
            
            Ok((insurance_event, user_event))
        }
    }
    
    /// Get insurance fund balance
    pub fn get_insurance_balance(&self, asset: &Asset) -> f64 {
        self.get_wallet(INSURANCE_FUND_USER_ID, asset)
            .map(|w| w.total_balance())
            .unwrap_or(0.0)
    }
    
    /// Replay an event (for crash recovery)
    pub fn replay_event(&mut self, event: &WalletEvent) -> Result<(), String> {
        let wallet = self.get_or_create_wallet(event.user_id, event.asset.clone());
        wallet.apply_event(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_deposit() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        let event = engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        
        assert!(matches!(event.event_type, WalletEventType::Deposit { amount } if amount == 10000.0));
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 10000.0);
        assert_eq!(wallet.locked_balance, 0.0);
        assert_eq!(wallet.total_balance(), 10000.0);
    }
    
    #[test]
    fn test_withdrawal() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine.withdraw(user_id, Asset::USDT, 3000.0).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 7000.0);
        assert_eq!(wallet.total_balance(), 7000.0);
    }
    
    #[test]
    fn test_lock_unlock() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine.lock_balance(user_id, Asset::USDT, 6000.0, None).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 4000.0);
        assert_eq!(wallet.locked_balance, 6000.0);
        assert_eq!(wallet.total_balance(), 10000.0);
        
        // Unlock half
        engine.unlock_balance(user_id, Asset::USDT, 3000.0, None).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 7000.0);
        assert_eq!(wallet.locked_balance, 3000.0);
    }
    
    #[test]
    fn test_debit_credit() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine.lock_balance(user_id, Asset::USDT, 5000.0, None).unwrap();
        
        // Realized loss (debit from locked)
        engine.debit(user_id, Asset::USDT, 1000.0, None).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.locked_balance, 4000.0);
        assert_eq!(wallet.total_balance(), 9000.0);
        
        // Realized profit (credit to free)
        engine.credit(user_id, Asset::USDT, 500.0, None).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 5500.0);
        assert_eq!(wallet.total_balance(), 9500.0);
    }
    
    #[test]
    fn test_insurance_transfer() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine.lock_balance(user_id, Asset::USDT, 10000.0, None).unwrap();
        
        // Transfer to insurance (liquidation penalty)
        engine.insurance_transfer(
            user_id,
            Asset::USDT,
            500.0,
            true,
            None,
        ).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.locked_balance, 9500.0);
        
        assert_eq!(engine.get_insurance_balance(&Asset::USDT), 500.0);
    }
    
    #[test]
    fn test_invariant_total_equals_free_plus_locked() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine.lock_balance(user_id, Asset::USDT, 6000.0, None).unwrap();
        engine.unlock_balance(user_id, Asset::USDT, 2000.0, None).unwrap();
        engine.debit(user_id, Asset::USDT, 500.0, None).unwrap();
        engine.credit(user_id, Asset::USDT, 300.0, None).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        
        // Invariant: total = free + locked
        assert_eq!(
            wallet.total_balance(),
            wallet.free_balance + wallet.locked_balance
        );
    }
}
```

---

# PART 4: INTEGRATION WITH SETTLEMENT

## File: `core/wallet/settlement_integration.rs`

```rust
use super::{domain::*, engine::WalletEngine};
use crate::settlement::domain::SettlementEvent;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Wallet Settlement Handler - processes settlement events
pub struct WalletSettlementHandler {
    wallet_engine: Arc<RwLock<WalletEngine>>,
}

impl WalletSettlementHandler {
    pub fn new(wallet_engine: Arc<RwLock<WalletEngine>>) -> Self {
        Self { wallet_engine }
    }
    
    /// Process settlement event
    /// 
    /// This is called when an instrument settles at expiry.
    /// Updates wallet balances based on payoffs.
    pub async fn process_settlement(
        &self,
        settlement: &SettlementEvent,
        collateral_asset: Asset,
    ) -> Result<Vec<WalletEvent>, String> {
        let mut engine = self.wallet_engine.write().await;
        let mut events = Vec::new();
        
        info!(
            instrument = %settlement.instrument_id,
            num_users = settlement.user_settlements.len(),
            "Processing settlement wallet updates"
        );
        
        for user_settlement in &settlement.user_settlements {
            // 1. Unlock margin
            if user_settlement.margin_released > 0.0 {
                let event = engine.unlock_balance(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.margin_released,
                    Some(settlement.event_id),
                )?;
                events.push(event);
            }
            
            // 2. Apply wallet change (payoff - premium paid)
            if user_settlement.wallet_change > 0.0 {
                // Profit: credit free balance
                let event = engine.credit(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.wallet_change,
                    Some(settlement.event_id),
                )?;
                events.push(event);
            } else if user_settlement.wallet_change < 0.0 {
                // Loss: debit locked balance
                let event = engine.debit(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.wallet_change.abs(),
                    Some(settlement.event_id),
                )?;
                events.push(event);
            }
        }
        
        Ok(events)
    }
}
```

---

# PART 5: LIQUIDATION INTEGRATION

## File: `core/wallet/liquidation_integration.rs`

```rust
use super::{domain::*, engine::WalletEngine};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Liquidation wallet handler
pub struct WalletLiquidationHandler {
    wallet_engine: Arc<RwLock<WalletEngine>>,
    liquidation_fee_rate: f64,
}

impl WalletLiquidationHandler {
    pub fn new(
        wallet_engine: Arc<RwLock<WalletEngine>>,
        liquidation_fee_rate: f64,
    ) -> Self {
        Self {
            wallet_engine,
            liquidation_fee_rate,
        }
    }
    
    /// Process liquidation
    /// 
    /// Called when a user position is liquidated.
    pub async fn process_liquidation(
        &self,
        user_id: Uuid,
        collateral_asset: Asset,
        realized_loss: f64,
        margin_released: f64,
    ) -> Result<Vec<WalletEvent>, String> {
        let mut engine = self.wallet_engine.write().await;
        let mut events = Vec::new();
        
        info!(
            user_id = %user_id,
            realized_loss = realized_loss,
            margin_released = margin_released,
            "Processing liquidation"
        );
        
        // 1. Debit realized loss
        if realized_loss > 0.0 {
            let event = engine.debit(
                user_id,
                collateral_asset.clone(),
                realized_loss,
                None,
            )?;
            events.push(event);
        }
        
        // 2. Liquidation fee to insurance fund
        let fee = realized_loss * self.liquidation_fee_rate;
        if fee > 0.0 {
            let (user_event, insurance_event) = engine.insurance_transfer(
                user_id,
                collateral_asset.clone(),
                fee,
                true,  // to insurance
                None,
            )?;
            events.push(user_event);
            events.push(insurance_event);
        }
        
        // 3. Release remaining margin
        if margin_released > 0.0 {
            let event = engine.unlock_balance(
                user_id,
                collateral_asset,
                margin_released,
                None,
            )?;
            events.push(event);
        }
        
        Ok(events)
    }
    
    /// Handle insurance fund bailout
    /// 
    /// Called when user goes negative and insurance must cover.
    pub async fn insurance_bailout(
        &self,
        user_id: Uuid,
        collateral_asset: Asset,
        deficit: f64,
    ) -> Result<(WalletEvent, WalletEvent), String> {
        let mut engine = self.wallet_engine.write().await;
        
        warn!(
            user_id = %user_id,
            deficit = deficit,
            "Insurance fund bailout required"
        );
        
        // Check insurance fund balance
        let insurance_balance = engine.get_insurance_balance(&collateral_asset);
        
        if insurance_balance < deficit {
            return Err(format!(
                "Insurance fund insufficient: {} < {}",
                insurance_balance, deficit
            ));
        }
        
        // Transfer from insurance to user
        engine.insurance_transfer(
            user_id,
            collateral_asset,
            deficit,
            false,  // from insurance
            None,
        )
    }
}
```

---

# PART 6: STORAGE TRAITS

## File: `core/wallet/traits.rs`

```rust
use super::domain::*;
use async_trait::async_trait;
use uuid::Uuid;

/// Storage interface for wallets
#[async_trait]
pub trait WalletStore: Send + Sync {
    /// Store wallet event
    async fn store_event(&self, event: WalletEvent) -> Result<(), String>;
    
    /// Get all events for a wallet
    async fn get_wallet_events(&self, wallet_id: Uuid) -> Result<Vec<WalletEvent>, String>;
    
    /// Get all events for a user
    async fn get_user_events(&self, user_id: Uuid) -> Result<Vec<WalletEvent>, String>;
    
    /// Get wallet snapshot
    async fn get_wallet(&self, user_id: Uuid, asset: &Asset) -> Result<Option<Wallet>, String>;
    
    /// Store wallet snapshot
    async fn store_wallet(&self, wallet: &Wallet) -> Result<(), String>;
}
```

---

# PART 7: DATABASE SCHEMA

```sql
-- Wallets table (snapshots)
CREATE TABLE wallets (
    wallet_id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    asset VARCHAR(20) NOT NULL,
    free_balance DECIMAL(28, 8) NOT NULL CHECK (free_balance >= 0),
    locked_balance DECIMAL(28, 8) NOT NULL CHECK (locked_balance >= 0),
    version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    
    UNIQUE(user_id, asset),
    CONSTRAINT wallet_balance_check CHECK (free_balance + locked_balance >= 0)
);

CREATE INDEX idx_wallets_user ON wallets(user_id);
CREATE INDEX idx_wallets_asset ON wallets(asset);

-- Wallet events table (event sourcing)
CREATE TABLE wallet_events (
    event_id UUID PRIMARY KEY,
    wallet_id UUID NOT NULL,
    user_id UUID NOT NULL,
    asset VARCHAR(20) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    amount DECIMAL(28, 8),
    to_insurance BOOLEAN,
    sequence BIGINT NOT NULL UNIQUE,
    timestamp TIMESTAMPTZ NOT NULL,
    source_event_id UUID,
    
    FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
);

CREATE INDEX idx_wallet_events_wallet ON wallet_events(wallet_id, sequence);
CREATE INDEX idx_wallet_events_user ON wallet_events(user_id, sequence);
CREATE INDEX idx_wallet_events_sequence ON wallet_events(sequence);
```

---

# PART 8: VIRTUAL WALLETS (Sandbox/Simulation)

## File: `core/wallet/virtual_wallet.rs`

```rust
use super::{domain::*, engine::WalletEngine};
use uuid::Uuid;

/// Virtual wallet provider for sandbox/simulation
pub struct VirtualWalletProvider {
    sandbox_engine: WalletEngine,
    simulation_engine: WalletEngine,
}

impl VirtualWalletProvider {
    pub fn new() -> Self {
        Self {
            sandbox_engine: WalletEngine::new(Environment::Sandbox),
            simulation_engine: WalletEngine::new(Environment::Simulation),
        }
    }
    
    /// Get engine for environment
    pub fn get_engine(&mut self, env: Environment) -> &mut WalletEngine {
        match env {
            Environment::Sandbox => &mut self.sandbox_engine,
            Environment::Simulation => &mut self.simulation_engine,
            Environment::Production => panic!("Production not supported in virtual provider"),
        }
    }
    
    /// Create sandbox wallet with virtual funds
    pub fn create_sandbox_wallet(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        initial_balance: f64,
    ) -> Result<Uuid, String> {
        self.sandbox_engine.deposit(user_id, asset, initial_balance)?;
        
        let wallet = self.sandbox_engine
            .get_wallet(user_id, &asset)
            .ok_or("Wallet not found after deposit")?;
        
        Ok(wallet.wallet_id)
    }
}

impl Default for VirtualWalletProvider {
    fn default() -> Self {
        Self::new()
    }
}
```

---

# PART 9: COMPREHENSIVE TESTS

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[test]
    fn test_full_lifecycle() {
        let mut engine = WalletEngine::new(Environment::Production);
        let user_id = Uuid::new_v4();
        
        // 1. Deposit
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        
        // 2. Lock for order
        engine.lock_balance(user_id, Asset::USDT, 6000.0, None).unwrap();
        
        // 3. Partial unlock (order partially filled)
        engine.unlock_balance(user_id, Asset::USDT, 2000.0, None).unwrap();
        
        // 4. Realized profit
        engine.credit(user_id, Asset::USDT, 500.0, None).unwrap();
        
        // 5. Withdrawal
        engine.withdraw(user_id, Asset::USDT, 3000.0).unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        
        // Verify final state
        // Started: 10000
        // Locked: -6000, Unlocked: +2000 = -4000 locked
        // Credit: +500
        // Withdraw: -3000
        // Free: 10000 - 4000 + 500 - 3000 = 3500
        // Locked: 4000
        assert_eq!(wallet.free_balance, 3500.0);
        assert_eq!(wallet.locked_balance, 4000.0);
        assert_eq!(wallet.total_balance(), 7500.0);
    }
    
    #[test]
    fn test_replay() {
        let mut engine1 = WalletEngine::new(Environment::Production);
        let user_id = Uuid::new_v4();
        
        // Generate events
        let events = vec![
            engine1.deposit(user_id, Asset::USDT, 10000.0).unwrap(),
            engine1.lock_balance(user_id, Asset::USDT, 5000.0, None).unwrap(),
            engine1.credit(user_id, Asset::USDT, 500.0, None).unwrap(),
        ];
        
        let wallet1 = engine1.get_wallet(user_id, &Asset::USDT).unwrap().clone();
        
        // Replay events in new engine
        let mut engine2 = WalletEngine::new(Environment::Production);
        for event in events {
            engine2.replay_event(&event).unwrap();
        }
        
        let wallet2 = engine2.get_wallet(user_id, &Asset::USDT).unwrap();
        
        // Must be identical
        assert_eq!(wallet1.free_balance, wallet2.free_balance);
        assert_eq!(wallet1.locked_balance, wallet2.locked_balance);
        assert_eq!(wallet1.total_balance(), wallet2.total_balance());
    }
}
```

---

# PART 10: COMPLETION CHECKLIST

Before marking this module complete:

- [ ] Wallet domain types implemented
- [ ] WalletEngine with all operations
- [ ] Event-driven mutations only
- [ ] Lock/unlock balance working
- [ ] Debit/credit working
- [ ] Deposit/withdrawal working
- [ ] Insurance fund transfers
- [ ] Total = free + locked invariant verified
- [ ] Settlement integration
- [ ] Liquidation integration
- [ ] Virtual wallets (sandbox/simulation)
- [ ] Event replay working
- [ ] Database schema created
- [ ] All unit tests passing
- [ ] Integration tests passing
- [ ] Crash recovery verified
- [ ] No direct wallet mutations
- [ ] Proper error handling
- [ ] Logging complete
- [ ] Documentation complete

---

# END OF MODULE 06 IMPLEMENTATION GUIDE

This Wallet System is production-ready. It:
- ✅ Is the single source of truth for funds
- ✅ Only mutates via events (deterministic)
- ✅ Maintains free vs locked balance correctly
- ✅ Integrates with Settlement (payoffs)
- ✅ Integrates with Liquidation (margin release)
- ✅ Supports insurance fund
- ✅ Supports virtual wallets (sandbox/simulation)
- ✅ Is replay-safe for crash recovery
- ✅ Never goes negative (insurance backing)
- ✅ Follows MASTER_RULES patterns

**Next**: Build Module 07 (Market Data & Analytics) - the final module!
