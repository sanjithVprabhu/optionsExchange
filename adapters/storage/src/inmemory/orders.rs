use async_trait::async_trait;
use exchange_oms::{Order, OrderError, OrderStore, RiskApproval, RiskClient};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// In-memory implementation of OrderStore.
///
/// Used for:
/// - Unit and integration testing (no database required)
/// - Development mode
/// - Config-driven selection: `storage.type: "inmemory"`
///
/// Thread-safe via tokio::sync::RwLock.
pub struct InMemoryOrderStore {
    orders: RwLock<HashMap<Uuid, Order>>,
}

impl InMemoryOrderStore {
    pub fn new() -> Self {
        Self {
            orders: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryOrderStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OrderStore for InMemoryOrderStore {
    async fn create_order(&self, order: Order) -> Result<Uuid, OrderError> {
        let id = order.order_id;
        let mut orders = self.orders.write().await;
        orders.insert(id, order);
        Ok(id)
    }

    async fn get_order(&self, order_id: Uuid) -> Result<Option<Order>, OrderError> {
        let orders = self.orders.read().await;
        Ok(orders.get(&order_id).cloned())
    }

    async fn update_order(&self, order: Order) -> Result<(), OrderError> {
        let mut orders = self.orders.write().await;
        orders.insert(order.order_id, order);
        Ok(())
    }

    async fn list_user_orders(&self, user_id: Uuid) -> Result<Vec<Order>, OrderError> {
        let orders = self.orders.read().await;
        let user_orders: Vec<_> = orders
            .values()
            .filter(|o| o.user_id == user_id)
            .cloned()
            .collect();
        Ok(user_orders)
    }

    async fn list_instrument_orders(&self, instrument_id: &str) -> Result<Vec<Order>, OrderError> {
        let orders = self.orders.read().await;
        let inst_orders: Vec<_> = orders
            .values()
            .filter(|o| o.instrument_id == instrument_id && o.status.is_active())
            .cloned()
            .collect();
        Ok(inst_orders)
    }

    async fn list_active_orders(&self, user_id: Uuid) -> Result<Vec<Order>, OrderError> {
        let orders = self.orders.read().await;
        let active: Vec<_> = orders
            .values()
            .filter(|o| o.user_id == user_id && o.status.is_active())
            .cloned()
            .collect();
        Ok(active)
    }
}

/// Mock Risk Client for testing.
///
/// Configurable to always approve or always reject orders.
/// Used in unit and integration tests.
pub struct MockRiskClient {
    should_approve: bool,
    rejection_reason: Option<String>,
}

impl MockRiskClient {
    /// Create a mock that always approves orders
    pub fn approving() -> Self {
        Self {
            should_approve: true,
            rejection_reason: None,
        }
    }

    /// Create a mock that always rejects orders
    pub fn rejecting(reason: impl Into<String>) -> Self {
        Self {
            should_approve: false,
            rejection_reason: Some(reason.into()),
        }
    }
}

#[async_trait]
impl RiskClient for MockRiskClient {
    async fn check_order_risk(
        &self,
        _order: &Order,
    ) -> std::result::Result<RiskApproval, OrderError> {
        Ok(RiskApproval {
            approved: self.should_approve,
            reason: self.rejection_reason.clone(),
            required_margin: if self.should_approve {
                Some(100.0)
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exchange_oms::{OrderBuilder, OrderSide, OrderStatus};

    fn make_test_order(user_id: Uuid) -> Order {
        OrderBuilder::new()
            .user_id(user_id)
            .instrument_id("test-instrument")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_order() {
        let store = InMemoryOrderStore::new();
        let user_id = Uuid::new_v4();
        let order = make_test_order(user_id);
        let order_id = order.order_id;

        let result = store.create_order(order).await.unwrap();
        assert_eq!(result, order_id);

        let retrieved = store.get_order(order_id).await.unwrap().unwrap();
        assert_eq!(retrieved.order_id, order_id);
        assert_eq!(retrieved.quantity, 10);
    }

    #[tokio::test]
    async fn test_update_order() {
        let store = InMemoryOrderStore::new();
        let user_id = Uuid::new_v4();
        let mut order = make_test_order(user_id);
        let order_id = order.order_id;

        store.create_order(order.clone()).await.unwrap();

        order.status = exchange_oms::OrderStatus::Open;
        store.update_order(order).await.unwrap();

        let retrieved = store.get_order(order_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, OrderStatus::Open);
    }

    #[tokio::test]
    async fn test_list_user_orders() {
        let store = InMemoryOrderStore::new();
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();

        store.create_order(make_test_order(user_a)).await.unwrap();
        store.create_order(make_test_order(user_a)).await.unwrap();
        store.create_order(make_test_order(user_b)).await.unwrap();

        let orders_a = store.list_user_orders(user_a).await.unwrap();
        assert_eq!(orders_a.len(), 2);

        let orders_b = store.list_user_orders(user_b).await.unwrap();
        assert_eq!(orders_b.len(), 1);
    }

    #[tokio::test]
    async fn test_list_active_orders() {
        let store = InMemoryOrderStore::new();
        let user_id = Uuid::new_v4();

        let mut order1 = make_test_order(user_id);
        order1.status = OrderStatus::Open;
        store.create_order(order1).await.unwrap();

        let mut order2 = make_test_order(user_id);
        order2.status = OrderStatus::Filled;
        store.create_order(order2).await.unwrap();

        let active = store.list_active_orders(user_id).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].status, OrderStatus::Open);
    }

    #[tokio::test]
    async fn test_list_instrument_orders() {
        let store = InMemoryOrderStore::new();
        let user_id = Uuid::new_v4();

        let mut order1 = make_test_order(user_id);
        order1.instrument_id = "INST-A".to_string();
        order1.status = OrderStatus::Open;
        store.create_order(order1).await.unwrap();

        let mut order2 = make_test_order(user_id);
        order2.instrument_id = "INST-B".to_string();
        order2.status = OrderStatus::Open;
        store.create_order(order2).await.unwrap();

        let inst_a = store.list_instrument_orders("INST-A").await.unwrap();
        assert_eq!(inst_a.len(), 1);
    }

    #[tokio::test]
    async fn test_get_nonexistent_order() {
        let store = InMemoryOrderStore::new();
        let result = store.get_order(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_mock_risk_client_approve() {
        let client = MockRiskClient::approving();
        let order = make_test_order(Uuid::new_v4());
        let approval = client.check_order_risk(&order).await.unwrap();
        assert!(approval.approved);
        assert!(approval.required_margin.is_some());
    }

    #[tokio::test]
    async fn test_mock_risk_client_reject() {
        let client = MockRiskClient::rejecting("Insufficient margin");
        let order = make_test_order(Uuid::new_v4());
        let approval = client.check_order_risk(&order).await.unwrap();
        assert!(!approval.approved);
        assert_eq!(approval.reason, Some("Insufficient margin".to_string()));
    }
}
