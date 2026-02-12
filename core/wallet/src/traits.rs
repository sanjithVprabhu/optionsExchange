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
    async fn get_wallet(
        &self,
        user_id: Uuid,
        asset: &Asset,
    ) -> Result<Option<Wallet>, String>;
    
    /// Store wallet snapshot
    async fn store_wallet(&self, wallet: &Wallet) -> Result<(), String>;
}
