use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::Order;
use crate::error::{OrderError, Result};

/// Storage interface for orders.
///
/// Implementations: InMemoryOrderStore, PostgresOrderStore, etc.
/// Core business logic uses this trait, NEVER a concrete database type.
#[async_trait]
pub trait OrderStore: Send + Sync {
    /// Persist a new order. Returns the order_id.
    async fn create_order(&self, order: Order) -> Result<Uuid>;

    /// Retrieve an order by its UUID.
    async fn get_order(&self, order_id: Uuid) -> Result<Option<Order>>;

    /// Update an existing order (status, fills, etc.)
    async fn update_order(&self, order: Order) -> Result<()>;

    /// List all orders for a given user.
    async fn list_user_orders(&self, user_id: Uuid) -> Result<Vec<Order>>;

    /// List all active (Open/PartiallyFilled) orders for an instrument.
    async fn list_instrument_orders(&self, instrument_id: &str) -> Result<Vec<Order>>;

    /// List all active (Open/PartiallyFilled) orders for a user.
    async fn list_active_orders(&self, user_id: Uuid) -> Result<Vec<Order>>;
}

/// Risk engine client interface.
///
/// OMS sends orders to the Risk Engine for approval before they become Open.
/// Core defines this trait; adapters implement it (gRPC, HTTP, mock, etc.)
#[async_trait]
pub trait RiskClient: Send + Sync {
    /// Check if order passes risk requirements.
    /// Returns RiskApproval with approved/rejected + reason.
    async fn check_order_risk(&self, order: &Order) -> std::result::Result<RiskApproval, OrderError>;
}

/// Risk approval result from the Risk Engine
#[derive(Debug, Clone)]
pub struct RiskApproval {
    /// Whether the order was approved
    pub approved: bool,
    /// Human-readable reason (especially for rejections)
    pub reason: Option<String>,
    /// Required margin for this order (if approved)
    pub required_margin: Option<f64>,
}
