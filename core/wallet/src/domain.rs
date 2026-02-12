use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
