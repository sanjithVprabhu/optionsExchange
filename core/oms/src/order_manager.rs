use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use exchange_instrument::InstrumentStore;

use crate::domain::{Order, OrderStatus};
use crate::error::{OrderError, Result};
use crate::traits::{OrderStore, RiskClient};
use crate::validation::{validate_order, OrderValidationConfig};

/// Order Manager - Core OMS business logic
///
/// Handles:
/// - Order submission (validate -> store -> risk check -> open/reject)
/// - Order cancellation (ownership check -> state check -> cancel)
/// - Fill application (update quantities and status)
/// - Order queries
///
/// Does NOT handle:
/// - Risk checking (delegates to RiskClient)
/// - Trade execution (delegates to Matching Engine)
/// - Balance/position mutations (Settlement does this)
pub struct OrderManager {
    order_store: Arc<dyn OrderStore>,
    instrument_store: Arc<dyn InstrumentStore>,
    risk_client: Arc<dyn RiskClient>,
    validation_config: OrderValidationConfig,
}

impl OrderManager {
    pub fn new(
        order_store: Arc<dyn OrderStore>,
        instrument_store: Arc<dyn InstrumentStore>,
        risk_client: Arc<dyn RiskClient>,
    ) -> Self {
        Self {
            order_store,
            instrument_store,
            risk_client,
            validation_config: OrderValidationConfig::default(),
        }
    }

    /// Create an OrderManager with custom validation config
    pub fn with_validation_config(
        order_store: Arc<dyn OrderStore>,
        instrument_store: Arc<dyn InstrumentStore>,
        risk_client: Arc<dyn RiskClient>,
        validation_config: OrderValidationConfig,
    ) -> Self {
        Self {
            order_store,
            instrument_store,
            risk_client,
            validation_config,
        }
    }

    /// Submit a new order.
    ///
    /// Flow:
    /// 1. Fetch the instrument and validate the order against it
    /// 2. Store the order as PendingRisk
    /// 3. Send to Risk Engine for approval
    /// 4. If approved, transition to Open
    /// 5. If rejected, transition to Rejected
    pub async fn submit_order(&self, mut order: Order) -> Result<Uuid> {
        info!(
            order_id = %order.order_id,
            user_id = %order.user_id,
            instrument_id = %order.instrument_id,
            side = %order.side,
            quantity = order.quantity,
            price = ?order.price,
            "Submitting order"
        );

        // 1. Get instrument
        let instrument = self
            .instrument_store
            .get_instrument(&order.instrument_id)
            .await
            .map_err(|e| OrderError::Storage(e.to_string()))?
            .ok_or_else(|| {
                OrderError::Storage(format!("Instrument not found: {}", order.instrument_id))
            })?;

        // 2. Validate order against instrument
        validate_order(&order, &instrument, &self.validation_config)?;

        // 3. Store with PendingRisk status
        order.status = OrderStatus::PendingRisk;
        let order_id = order.order_id;
        self.order_store.create_order(order.clone()).await?;

        // 4. Check risk
        let risk_approval = self.risk_client.check_order_risk(&order).await?;

        if risk_approval.approved {
            // 5a. Transition to Open
            order.transition_to(OrderStatus::Open).map_err(|_| {
                OrderError::InvalidTransition {
                    from: order.status,
                    to: OrderStatus::Open,
                }
            })?;
            self.order_store.update_order(order).await?;

            info!(order_id = %order_id, "Order approved by risk engine");
        } else {
            // 5b. Risk rejected
            order.transition_to(OrderStatus::Rejected).map_err(|_| {
                OrderError::InvalidTransition {
                    from: order.status,
                    to: OrderStatus::Rejected,
                }
            })?;
            self.order_store.update_order(order).await?;

            warn!(
                order_id = %order_id,
                reason = ?risk_approval.reason,
                "Order rejected by risk engine"
            );
        }

        Ok(order_id)
    }

    /// Cancel an order.
    ///
    /// Only the owner can cancel their own orders.
    /// Only active orders (Open/PartiallyFilled) can be cancelled.
    pub async fn cancel_order(&self, order_id: Uuid, user_id: Uuid) -> Result<()> {
        // Get order
        let mut order = self
            .order_store
            .get_order(order_id)
            .await?
            .ok_or(OrderError::NotFound(order_id))?;

        // Verify ownership
        if order.user_id != user_id {
            return Err(OrderError::NotAuthorized(
                "Cannot cancel another user's order".to_string(),
            ));
        }

        // Only cancel if active
        if !order.status.is_active() {
            return Err(OrderError::InvalidTransition {
                from: order.status,
                to: OrderStatus::Cancelled,
            });
        }

        // Transition to Cancelled
        order
            .transition_to(OrderStatus::Cancelled)
            .map_err(|_| OrderError::InvalidTransition {
                from: order.status,
                to: OrderStatus::Cancelled,
            })?;
        self.order_store.update_order(order).await?;

        info!(order_id = %order_id, "Order cancelled");

        Ok(())
    }

    /// Apply a fill to an order (called by Matching Engine via events).
    ///
    /// Updates filled_quantity, avg_fill_price, and status.
    /// Does NOT mutate balances or positions.
    pub async fn apply_fill(
        &self,
        order_id: Uuid,
        fill_quantity: u32,
        fill_price: f64,
    ) -> Result<()> {
        let mut order = self
            .order_store
            .get_order(order_id)
            .await?
            .ok_or(OrderError::NotFound(order_id))?;

        // Apply fill (updates status internally)
        order.apply_fill(fill_quantity, fill_price);

        // Persist updated order
        self.order_store.update_order(order.clone()).await?;

        info!(
            order_id = %order_id,
            filled = fill_quantity,
            price = fill_price,
            total_filled = order.filled_quantity,
            remaining = order.remaining_quantity(),
            status = %order.status,
            "Fill applied"
        );

        Ok(())
    }

    /// Get a single order by ID.
    pub async fn get_order(&self, order_id: Uuid) -> Result<Order> {
        self.order_store
            .get_order(order_id)
            .await?
            .ok_or(OrderError::NotFound(order_id))
    }

    /// List all orders for a user.
    pub async fn list_user_orders(&self, user_id: Uuid) -> Result<Vec<Order>> {
        self.order_store.list_user_orders(user_id).await
    }

    /// List active orders (Open/PartiallyFilled) for a user.
    pub async fn list_active_orders(&self, user_id: Uuid) -> Result<Vec<Order>> {
        self.order_store.list_active_orders(user_id).await
    }

    /// List active orders for an instrument.
    pub async fn list_instrument_orders(&self, instrument_id: &str) -> Result<Vec<Order>> {
        self.order_store.list_instrument_orders(instrument_id).await
    }
}
