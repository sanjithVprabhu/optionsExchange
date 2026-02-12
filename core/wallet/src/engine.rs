use super::domain::*;
use std::collections::HashMap;
use tracing::{error, info, warn};
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
        let wallet = self
            .wallets
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
                WalletEventType::InsuranceTransfer {
                    amount,
                    to_insurance: true,
                },
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
                WalletEventType::InsuranceTransfer {
                    amount,
                    to_insurance: false,
                },
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
                WalletEventType::InsuranceTransfer {
                    amount,
                    to_insurance: true,
                },
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
                WalletEventType::InsuranceTransfer {
                    amount,
                    to_insurance: false,
                },
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
        engine
            .lock_balance(user_id, Asset::USDT, 6000.0, None)
            .unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 4000.0);
        assert_eq!(wallet.locked_balance, 6000.0);
        assert_eq!(wallet.total_balance(), 10000.0);
        
        // Unlock half
        engine
            .unlock_balance(user_id, Asset::USDT, 3000.0, None)
            .unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.free_balance, 7000.0);
        assert_eq!(wallet.locked_balance, 3000.0);
    }
    
    #[test]
    fn test_debit_credit() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine
            .lock_balance(user_id, Asset::USDT, 5000.0, None)
            .unwrap();
        
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
        engine
            .lock_balance(user_id, Asset::USDT, 10000.0, None)
            .unwrap();
        
        // Transfer to insurance (liquidation penalty)
        engine
            .insurance_transfer(user_id, Asset::USDT, 500.0, true, None)
            .unwrap();
        
        let wallet = engine.get_wallet(user_id, &Asset::USDT).unwrap();
        assert_eq!(wallet.locked_balance, 9500.0);
        
        assert_eq!(engine.get_insurance_balance(&Asset::USDT), 500.0);
    }
    
    #[test]
    fn test_invariant_total_equals_free_plus_locked() {
        let mut engine = WalletEngine::new(Environment::Sandbox);
        let user_id = Uuid::new_v4();
        
        engine.deposit(user_id, Asset::USDT, 10000.0).unwrap();
        engine
            .lock_balance(user_id, Asset::USDT, 6000.0, None)
            .unwrap();
        engine
            .unlock_balance(user_id, Asset::USDT, 2000.0, None)
            .unwrap();
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
