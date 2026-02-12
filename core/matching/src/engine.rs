use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

use exchange_oms::{OrderSide, TimeInForce};

use crate::domain::*;

/// Matching Engine - The heart of the exchange.
///
/// CRITICAL PROPERTIES:
/// 1. Deterministic (same inputs -> same outputs, always)
/// 2. Pure function (no external state, no side effects beyond book mutation)
/// 3. Price-time priority (strictly enforced)
/// 4. Per-instrument isolation (books never interact)
///
/// The matching engine does NOT:
/// - Check balances or margin
/// - Use system time for ordering decisions
/// - Make external calls
/// - Validate orders (OMS does that)
pub struct MatchingEngine {
    /// Order books per instrument
    books: HashMap<String, OrderBook>,
    /// Global sequence counter (monotonic, never resets)
    sequence: u64,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
            sequence: 0,
        }
    }

    /// Get or create order book for instrument
    fn get_or_create_book(&mut self, instrument_id: &str) -> &mut OrderBook {
        self.books
            .entry(instrument_id.to_string())
            .or_insert_with(|| OrderBook::new(instrument_id.to_string()))
    }

    /// Submit a new order into the matching engine.
    ///
    /// This is the main entry point. It:
    /// 1. Checks FOK feasibility (pre-check)
    /// 2. Matches against the opposite side of the book
    /// 3. Handles remainder based on time-in-force
    /// 4. Inserts remainder into book if GTC
    /// 5. Returns trades and match result
    pub fn submit_order(&mut self, order: BookOrder) -> MatchResult {
        info!(
            order_id = %order.order_id,
            instrument = %order.instrument_id,
            side = %order.side,
            price = order.price,
            quantity = order.quantity,
            tif = %order.time_in_force,
            "Submitting order to matching engine"
        );

        let instrument_id = order.instrument_id.clone();

        // FOK pre-check: verify full quantity can be filled before touching the book
        if order.time_in_force == TimeInForce::FOK {
            let book = self.get_or_create_book(&instrument_id);
            let available = match order.side {
                OrderSide::Buy => book.available_ask_quantity_at_or_below(order.price),
                OrderSide::Sell => book.available_bid_quantity_at_or_above(order.price),
            };

            if available < order.quantity {
                info!(
                    order_id = %order.order_id,
                    available = available,
                    required = order.quantity,
                    "FOK order rejected: insufficient liquidity"
                );
                return MatchResult::cancelled(order);
            }
        }

        // Match the order against the opposite side
        let result = match order.side {
            OrderSide::Buy => self.match_buy(order),
            OrderSide::Sell => self.match_sell(order),
        };

        result
    }

    /// Match a buy order against asks (sell side).
    ///
    /// Walks the ask ladder from lowest price upward.
    /// At each price level, matches FIFO (time priority).
    /// Generates atomic trades at the maker's price.
    fn match_buy(&mut self, mut order: BookOrder) -> MatchResult {
        let mut trades = Vec::new();
        let instrument_id = order.instrument_id.clone();

        // Use a block to scope the book borrow so finalize_result can borrow self later
        {
            let book = self.books
                .entry(instrument_id.clone())
                .or_insert_with(|| OrderBook::new(instrument_id.clone()));

            loop {
                // Get the best (lowest) ask price
                let best_ask_price = match book.asks.keys().next() {
                    Some(price) => price.into_inner(),
                    None => break, // No sellers
                };

                // Check if price crosses (buy price >= ask price)
                if best_ask_price > order.price {
                    break; // No more matches at acceptable price
                }

                let price_key = ordered_float::OrderedFloat(best_ask_price);

                // Pop the front maker order; remove empty price levels to avoid infinite loop
                let mut maker_order = match book.asks.get_mut(&price_key).and_then(|q| q.pop_front()) {
                    Some(o) => o,
                    None => {
                        book.asks.remove(&price_key);
                        continue;
                    }
                };

                let trade_qty = order.quantity.min(maker_order.quantity);

                // Increment sequence via direct field access (split borrow)
                self.sequence += 1;

                // Generate trade at MAKER's price (critical!)
                let trade = Trade::new(
                    instrument_id.clone(),
                    order.order_id,        // Taker (aggressor)
                    maker_order.order_id,  // Maker (resting)
                    order.user_id,         // Buyer
                    maker_order.user_id,   // Seller
                    maker_order.price,     // MAKER PRICE
                    trade_qty,
                    OrderSide::Buy,        // Aggressor side
                    self.sequence,
                );

                debug!(
                    trade_id = %trade.trade_id,
                    price = trade.price,
                    quantity = trade.quantity,
                    maker = %trade.maker_order_id,
                    "Trade executed (buy aggressor)"
                );

                trades.push(trade);

                // Update quantities
                order.fill(trade_qty);
                maker_order.fill(trade_qty);

                // If maker order not fully filled, put it back at the front
                if !maker_order.is_filled() {
                    if let Some(q) = book.asks.get_mut(&price_key) {
                        q.push_front(maker_order);
                    }
                }

                // If our order is fully filled, we're done
                if order.is_filled() {
                    break;
                }
            }

            // Clean up empty price levels
            book.cleanup_empty_levels();
        }

        // Handle remainder based on time-in-force
        self.finalize_result(trades, order)
    }

    /// Match a sell order against bids (buy side).
    ///
    /// Walks the bid ladder from highest price downward.
    /// At each price level, matches FIFO (time priority).
    /// Generates atomic trades at the maker's price.
    fn match_sell(&mut self, mut order: BookOrder) -> MatchResult {
        let mut trades = Vec::new();
        let instrument_id = order.instrument_id.clone();

        // Use a block to scope the book borrow so finalize_result can borrow self later
        {
            let book = self.books
                .entry(instrument_id.clone())
                .or_insert_with(|| OrderBook::new(instrument_id.clone()));

            loop {
                // Get the best (highest) bid price
                let best_bid_price = match book.bids.keys().next() {
                    Some(reverse_price) => reverse_price.0.into_inner(),
                    None => break, // No buyers
                };

                // Check if price crosses (sell price <= bid price)
                if best_bid_price < order.price {
                    break; // No more matches at acceptable price
                }

                let price_key = std::cmp::Reverse(ordered_float::OrderedFloat(best_bid_price));

                // Pop the front maker order; remove empty price levels to avoid infinite loop
                let mut maker_order = match book.bids.get_mut(&price_key).and_then(|q| q.pop_front()) {
                    Some(o) => o,
                    None => {
                        book.bids.remove(&price_key);
                        continue;
                    }
                };

                let trade_qty = order.quantity.min(maker_order.quantity);

                // Increment sequence via direct field access (split borrow)
                self.sequence += 1;

                // Generate trade at MAKER's price (critical!)
                let trade = Trade::new(
                    instrument_id.clone(),
                    order.order_id,        // Taker (aggressor)
                    maker_order.order_id,  // Maker (resting)
                    maker_order.user_id,   // Buyer
                    order.user_id,         // Seller
                    maker_order.price,     // MAKER PRICE
                    trade_qty,
                    OrderSide::Sell,       // Aggressor side
                    self.sequence,
                );

                debug!(
                    trade_id = %trade.trade_id,
                    price = trade.price,
                    quantity = trade.quantity,
                    maker = %trade.maker_order_id,
                    "Trade executed (sell aggressor)"
                );

                trades.push(trade);

                // Update quantities
                order.fill(trade_qty);
                maker_order.fill(trade_qty);

                // If maker order not fully filled, put it back at the front
                if !maker_order.is_filled() {
                    if let Some(q) = book.bids.get_mut(&price_key) {
                        q.push_front(maker_order);
                    }
                }

                // If our order is fully filled, we're done
                if order.is_filled() {
                    break;
                }
            }

            // Clean up empty price levels
            book.cleanup_empty_levels();
        }

        // Handle remainder based on time-in-force
        self.finalize_result(trades, order)
    }

    /// Finalize the match result: decide whether to insert remainder into book
    fn finalize_result(&mut self, trades: Vec<Trade>, order: BookOrder) -> MatchResult {
        if order.is_filled() {
            return MatchResult::fully_matched(trades);
        }

        match order.time_in_force {
            TimeInForce::GTC => {
                // Insert remainder into book
                let instrument_id = order.instrument_id.clone();
                let book = self.get_or_create_book(&instrument_id);
                book.insert_order(order.clone());
                if trades.is_empty() {
                    MatchResult::no_match(order, true)
                } else {
                    MatchResult::partial_match(trades, order, true)
                }
            }
            TimeInForce::IOC => {
                // Cancel remainder (don't insert)
                if trades.is_empty() {
                    MatchResult::cancelled(order)
                } else {
                    MatchResult::partial_match(trades, order, false)
                }
            }
            TimeInForce::FOK => {
                // FOK should have been pre-checked. If we get here with remainder,
                // something unexpected happened. Cancel remainder for safety.
                MatchResult::cancelled(order)
            }
        }
    }

    /// Cancel an order from the book
    pub fn cancel_order(&mut self, instrument_id: &str, order_id: Uuid) -> Option<BookOrder> {
        if let Some(book) = self.books.get_mut(instrument_id) {
            let result = book.remove_order(order_id);
            book.cleanup_empty_levels();
            result
        } else {
            None
        }
    }

    /// Get a reference to an order book (for snapshots/queries)
    pub fn get_book(&self, instrument_id: &str) -> Option<&OrderBook> {
        self.books.get(instrument_id)
    }

    /// Get current global sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence
    }
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_book_order(
        instrument_id: &str,
        side: OrderSide,
        price: f64,
        quantity: u32,
        tif: TimeInForce,
    ) -> BookOrder {
        BookOrder {
            order_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            instrument_id: instrument_id.to_string(),
            side,
            price,
            quantity,
            sequence: 0,
            time_in_force: tif,
        }
    }

    const INST: &str = "BTC-28MAR2026-50000-C";

    // ========================================================================
    // BASIC MATCHING
    // ========================================================================

    #[test]
    fn test_no_match_empty_book() {
        let mut engine = MatchingEngine::new();

        // Sell into empty book -> no match, insert
        let sell = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(sell);

        assert_eq!(result.trades.len(), 0);
        assert!(result.inserted);
        assert!(result.remaining_order.is_some());

        // Verify it's in the book
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.ask_order_count(), 1);
    }

    #[test]
    fn test_basic_full_match() {
        let mut engine = MatchingEngine::new();

        // Sell 10 @ 100
        let sell = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        engine.submit_order(sell);

        // Buy 10 @ 100 -> full match
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 10);
        assert_eq!(result.trades[0].price, 100.0);
        assert!(result.remaining_order.is_none());
        assert!(!result.inserted);

        // Book should be empty
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.ask_order_count(), 0);
        assert_eq!(book.bid_order_count(), 0);
    }

    #[test]
    fn test_buy_aggressor_trade_fields() {
        let mut engine = MatchingEngine::new();

        let seller_id = Uuid::new_v4();
        let sell = BookOrder {
            order_id: Uuid::new_v4(),
            user_id: seller_id,
            instrument_id: INST.to_string(),
            side: OrderSide::Sell,
            price: 100.0,
            quantity: 10,
            sequence: 0,
            time_in_force: TimeInForce::GTC,
        };
        let sell_order_id = sell.order_id;
        engine.submit_order(sell);

        let buyer_id = Uuid::new_v4();
        let buy = BookOrder {
            order_id: Uuid::new_v4(),
            user_id: buyer_id,
            instrument_id: INST.to_string(),
            side: OrderSide::Buy,
            price: 105.0, // willing to pay more
            quantity: 10,
            sequence: 0,
            time_in_force: TimeInForce::GTC,
        };
        let buy_order_id = buy.order_id;
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!(trade.taker_order_id, buy_order_id);
        assert_eq!(trade.maker_order_id, sell_order_id);
        assert_eq!(trade.buyer_id, buyer_id);
        assert_eq!(trade.seller_id, seller_id);
        assert_eq!(trade.price, 100.0); // Maker's price, not 105
        assert_eq!(trade.aggressor_side, OrderSide::Buy);
    }

    #[test]
    fn test_sell_aggressor_trade_fields() {
        let mut engine = MatchingEngine::new();

        let buyer_id = Uuid::new_v4();
        let buy = BookOrder {
            order_id: Uuid::new_v4(),
            user_id: buyer_id,
            instrument_id: INST.to_string(),
            side: OrderSide::Buy,
            price: 100.0,
            quantity: 10,
            sequence: 0,
            time_in_force: TimeInForce::GTC,
        };
        let buy_order_id = buy.order_id;
        engine.submit_order(buy);

        let seller_id = Uuid::new_v4();
        let sell = BookOrder {
            order_id: Uuid::new_v4(),
            user_id: seller_id,
            instrument_id: INST.to_string(),
            side: OrderSide::Sell,
            price: 95.0, // willing to sell cheaper
            quantity: 10,
            sequence: 0,
            time_in_force: TimeInForce::GTC,
        };
        let sell_order_id = sell.order_id;
        let result = engine.submit_order(sell);

        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!(trade.taker_order_id, sell_order_id);
        assert_eq!(trade.maker_order_id, buy_order_id);
        assert_eq!(trade.buyer_id, buyer_id);
        assert_eq!(trade.seller_id, seller_id);
        assert_eq!(trade.price, 100.0); // Maker's price, not 95
        assert_eq!(trade.aggressor_side, OrderSide::Sell);
    }

    // ========================================================================
    // PARTIAL FILLS
    // ========================================================================

    #[test]
    fn test_partial_fill_buy() {
        let mut engine = MatchingEngine::new();

        // Sell 5 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 5, TimeInForce::GTC));

        // Buy 10 @ 100 -> partial fill (5 of 10)
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 5);
        assert!(result.remaining_order.is_some());
        assert_eq!(result.remaining_order.as_ref().unwrap().quantity, 5);
        assert!(result.inserted); // GTC remainder inserted

        // 5 remaining should be in bid book
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 1);
        assert_eq!(book.bid_quantity_at(100.0), 5);
    }

    #[test]
    fn test_partial_fill_sell() {
        let mut engine = MatchingEngine::new();

        // Buy 5 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Buy, 100.0, 5, TimeInForce::GTC));

        // Sell 10 @ 100 -> partial fill (5 of 10)
        let sell = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(sell);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 5);
        assert_eq!(result.remaining_order.as_ref().unwrap().quantity, 5);
        assert!(result.inserted);

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.ask_order_count(), 1);
        assert_eq!(book.ask_quantity_at(100.0), 5);
    }

    #[test]
    fn test_match_across_multiple_price_levels() {
        let mut engine = MatchingEngine::new();

        // Sell 4 @ 99, Sell 7 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 99.0, 4, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 7, TimeInForce::GTC));

        // Buy 10 @ 100 -> matches both levels
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        // 2 trades: 4 @ 99, 6 @ 100
        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].price, 99.0);
        assert_eq!(result.trades[0].quantity, 4);
        assert_eq!(result.trades[1].price, 100.0);
        assert_eq!(result.trades[1].quantity, 6);
        assert!(result.remaining_order.is_none()); // fully filled

        // 1 ask remaining (1 @ 100)
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.ask_quantity_at(100.0), 1);
    }

    // ========================================================================
    // PRICE-TIME PRIORITY (FIFO)
    // ========================================================================

    #[test]
    fn test_fifo_priority() {
        let mut engine = MatchingEngine::new();

        // 3 sells at same price, different "times"
        let sell1 = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let sell2 = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let sell3 = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let sell1_id = sell1.order_id;
        let sell2_id = sell2.order_id;

        engine.submit_order(sell1);
        engine.submit_order(sell2);
        engine.submit_order(sell3);

        // Buy 15 -> should match sell1 fully (10), sell2 partially (5)
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 15, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].maker_order_id, sell1_id); // First in = first matched
        assert_eq!(result.trades[0].quantity, 10);
        assert_eq!(result.trades[1].maker_order_id, sell2_id);
        assert_eq!(result.trades[1].quantity, 5);
    }

    #[test]
    fn test_price_priority_over_time() {
        let mut engine = MatchingEngine::new();

        // Sell @ 101 first, then sell @ 100 (better price)
        let sell_101 = make_book_order(INST, OrderSide::Sell, 101.0, 10, TimeInForce::GTC);
        let sell_100 = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC);
        let sell_100_id = sell_100.order_id;

        engine.submit_order(sell_101);
        engine.submit_order(sell_100);

        // Buy @ 101 -> should match sell @ 100 first (better price)
        let buy = make_book_order(INST, OrderSide::Buy, 101.0, 5, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].maker_order_id, sell_100_id);
        assert_eq!(result.trades[0].price, 100.0);
    }

    // ========================================================================
    // TIME-IN-FORCE
    // ========================================================================

    #[test]
    fn test_gtc_inserts_remainder() {
        let mut engine = MatchingEngine::new();

        // No liquidity, GTC buy -> inserts into book
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(result.inserted);

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 1);
    }

    #[test]
    fn test_ioc_partial_fill_no_insert() {
        let mut engine = MatchingEngine::new();

        // Sell 5 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 5, TimeInForce::GTC));

        // IOC Buy 10 @ 100 -> fills 5, cancels 5 (no insert)
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::IOC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 5);
        assert!(!result.inserted);
        assert_eq!(result.remaining_order.as_ref().unwrap().quantity, 5);

        // Nothing in bid book (IOC doesn't rest)
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 0);
    }

    #[test]
    fn test_ioc_no_match_cancelled() {
        let mut engine = MatchingEngine::new();

        // IOC Buy with no sellers -> cancelled
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::IOC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(!result.inserted);

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 0);
    }

    #[test]
    fn test_fok_full_fill() {
        let mut engine = MatchingEngine::new();

        // Sell 10 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC));

        // FOK Buy 10 @ 100 -> full fill (enough liquidity)
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::FOK);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 10);
        assert!(result.remaining_order.is_none());
    }

    #[test]
    fn test_fok_insufficient_liquidity_cancelled() {
        let mut engine = MatchingEngine::new();

        // Sell 5 @ 100 (not enough for 10)
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 5, TimeInForce::GTC));

        // FOK Buy 10 @ 100 -> cancelled (can't fill all 10)
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::FOK);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(!result.inserted);

        // Original sell order should still be in the book (untouched)
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.ask_quantity_at(100.0), 5);
    }

    #[test]
    fn test_fok_no_liquidity_cancelled() {
        let mut engine = MatchingEngine::new();

        // FOK Buy with no sellers -> cancelled
        let buy = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::FOK);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(!result.inserted);
    }

    #[test]
    fn test_fok_sell_insufficient() {
        let mut engine = MatchingEngine::new();

        // Buy 5 @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Buy, 100.0, 5, TimeInForce::GTC));

        // FOK Sell 10 @ 100 -> cancelled
        let sell = make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::FOK);
        let result = engine.submit_order(sell);

        assert_eq!(result.trades.len(), 0);
        assert!(!result.inserted);

        // Original buy still in book
        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_quantity_at(100.0), 5);
    }

    // ========================================================================
    // PRICE CROSSING
    // ========================================================================

    #[test]
    fn test_no_crossing_buy_below_ask() {
        let mut engine = MatchingEngine::new();

        // Sell @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 10, TimeInForce::GTC));

        // Buy @ 99 -> no crossing, inserts into bid book
        let buy = make_book_order(INST, OrderSide::Buy, 99.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(result.inserted);

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 1);
        assert_eq!(book.ask_order_count(), 1);
    }

    #[test]
    fn test_no_crossing_sell_above_bid() {
        let mut engine = MatchingEngine::new();

        // Buy @ 100
        engine.submit_order(make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC));

        // Sell @ 101 -> no crossing, inserts into ask book
        let sell = make_book_order(INST, OrderSide::Sell, 101.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(sell);

        assert_eq!(result.trades.len(), 0);
        assert!(result.inserted);

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.bid_order_count(), 1);
        assert_eq!(book.ask_order_count(), 1);
        assert_eq!(book.spread(), Some(1.0));
    }

    // ========================================================================
    // CANCELLATION
    // ========================================================================

    #[test]
    fn test_cancel_order() {
        let mut engine = MatchingEngine::new();

        let order = make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let order_id = order.order_id;
        engine.submit_order(order);

        assert_eq!(engine.get_book(INST).unwrap().bid_order_count(), 1);

        let cancelled = engine.cancel_order(INST, order_id);
        assert!(cancelled.is_some());
        assert_eq!(cancelled.unwrap().order_id, order_id);

        assert_eq!(engine.get_book(INST).unwrap().bid_order_count(), 0);
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let mut engine = MatchingEngine::new();
        let result = engine.cancel_order(INST, Uuid::new_v4());
        assert!(result.is_none());
    }

    // ========================================================================
    // ORDER BOOK QUERIES
    // ========================================================================

    #[test]
    fn test_best_bid_ask_spread() {
        let mut engine = MatchingEngine::new();

        engine.submit_order(make_book_order(INST, OrderSide::Buy, 99.0, 10, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 101.0, 10, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 102.0, 10, TimeInForce::GTC));

        let book = engine.get_book(INST).unwrap();
        assert_eq!(book.best_bid(), Some(100.0));
        assert_eq!(book.best_ask(), Some(101.0));
        assert_eq!(book.spread(), Some(1.0));
    }

    #[test]
    fn test_order_book_snapshot() {
        let mut engine = MatchingEngine::new();

        engine.submit_order(make_book_order(INST, OrderSide::Buy, 99.0, 5, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Buy, 100.0, 10, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 101.0, 8, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 102.0, 3, TimeInForce::GTC));

        let book = engine.get_book(INST).unwrap();
        let snapshot = OrderBookSnapshot::from_book(book);

        assert_eq!(snapshot.instrument_id, INST);
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.asks.len(), 2);

        // Bids descending
        assert_eq!(snapshot.bids[0].price, 100.0);
        assert_eq!(snapshot.bids[0].quantity, 10);
        assert_eq!(snapshot.bids[1].price, 99.0);
        assert_eq!(snapshot.bids[1].quantity, 5);

        // Asks ascending
        assert_eq!(snapshot.asks[0].price, 101.0);
        assert_eq!(snapshot.asks[0].quantity, 8);
        assert_eq!(snapshot.asks[1].price, 102.0);
        assert_eq!(snapshot.asks[1].quantity, 3);
    }

    // ========================================================================
    // INSTRUMENT ISOLATION
    // ========================================================================

    #[test]
    fn test_instrument_isolation() {
        let mut engine = MatchingEngine::new();

        let inst_a = "INST-A";
        let inst_b = "INST-B";

        // Sell on instrument A
        engine.submit_order(make_book_order(inst_a, OrderSide::Sell, 100.0, 10, TimeInForce::GTC));

        // Buy on instrument B -> no match (different instrument)
        let buy = make_book_order(inst_b, OrderSide::Buy, 100.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 0);
        assert!(result.inserted);

        // Each instrument has its own book
        assert_eq!(engine.get_book(inst_a).unwrap().ask_order_count(), 1);
        assert_eq!(engine.get_book(inst_b).unwrap().bid_order_count(), 1);
    }

    // ========================================================================
    // DETERMINISM
    // ========================================================================

    #[test]
    fn test_determinism_same_inputs_same_outputs() {
        // Create deterministic orders with fixed UUIDs
        let user_a = Uuid::from_u128(1);
        let user_b = Uuid::from_u128(2);
        let user_c = Uuid::from_u128(3);

        let make_orders = || {
            vec![
                BookOrder {
                    order_id: Uuid::from_u128(100),
                    user_id: user_a,
                    instrument_id: INST.to_string(),
                    side: OrderSide::Sell,
                    price: 100.0,
                    quantity: 10,
                    sequence: 0,
                    time_in_force: TimeInForce::GTC,
                },
                BookOrder {
                    order_id: Uuid::from_u128(101),
                    user_id: user_b,
                    instrument_id: INST.to_string(),
                    side: OrderSide::Sell,
                    price: 99.0,
                    quantity: 5,
                    sequence: 0,
                    time_in_force: TimeInForce::GTC,
                },
                BookOrder {
                    order_id: Uuid::from_u128(102),
                    user_id: user_c,
                    instrument_id: INST.to_string(),
                    side: OrderSide::Buy,
                    price: 100.0,
                    quantity: 12,
                    sequence: 0,
                    time_in_force: TimeInForce::GTC,
                },
            ]
        };

        // Run 1
        let mut engine1 = MatchingEngine::new();
        let mut results1: Vec<Vec<(f64, u32)>> = Vec::new();
        for order in make_orders() {
            let r = engine1.submit_order(order);
            results1.push(r.trades.iter().map(|t| (t.price, t.quantity)).collect());
        }

        // Run 2
        let mut engine2 = MatchingEngine::new();
        let mut results2: Vec<Vec<(f64, u32)>> = Vec::new();
        for order in make_orders() {
            let r = engine2.submit_order(order);
            results2.push(r.trades.iter().map(|t| (t.price, t.quantity)).collect());
        }

        // Must be identical
        assert_eq!(results1, results2);
    }

    // ========================================================================
    // SEQUENCE NUMBERS
    // ========================================================================

    #[test]
    fn test_sequence_numbers_monotonic() {
        let mut engine = MatchingEngine::new();

        // Create crossing orders to generate trades
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 100.0, 5, TimeInForce::GTC));
        engine.submit_order(make_book_order(INST, OrderSide::Sell, 101.0, 5, TimeInForce::GTC));

        let buy = make_book_order(INST, OrderSide::Buy, 101.0, 10, TimeInForce::GTC);
        let result = engine.submit_order(buy);

        assert_eq!(result.trades.len(), 2);
        // Sequence numbers must be strictly increasing
        assert!(result.trades[0].sequence < result.trades[1].sequence);
    }
}
