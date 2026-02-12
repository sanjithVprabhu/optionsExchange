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
            Environment::Production => {
                panic!("Production not supported in virtual provider")
            }
        }
    }
    
    /// Create sandbox wallet with virtual funds
    pub fn create_sandbox_wallet(
        &mut self,
        user_id: Uuid,
        asset: Asset,
        initial_balance: f64,
    ) -> Result<Uuid, String> {
        self.sandbox_engine.deposit(user_id, asset.clone(), initial_balance)?;
        
        let wallet = self
            .sandbox_engine
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
