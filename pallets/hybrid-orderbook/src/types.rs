
use super::*;

/// Identifier for each order. Highest bit of the order id is used to indicate the order type (0 = bid, 1 = ask).
pub type OrderId = u64;

/// Type alias for the orderbook.
pub type OrderBook<K, V> = CritbitTree<K, V>;

/// Type alias for the orders of the orderbook.
pub type Orders<Amount, Account, BlockNumber> = Vec<Order<Amount, Account, BlockNumber>>;

/// The order of the orderbook.
#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
pub struct Order<Amount, Account, BlockNumber> {
    order_id: OrderId,
    quantity: Amount,
    owner: Account,
    expired_at: BlockNumber,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
pub struct Pool<K, V> {
    /// The amount of the base asset.
    base_amount: K,
    /// The amount of the quote asset.
    quote_amount: K,
    /// The orderbook of the bid.
    bids: OrderBook<K, V>,
    /// The orderbook of the ask.
    asks: OrderBook<K, V>,
    // Orderbook
    next_bid_order_id: OrderId,
    /// The next order id of the ask.
    next_ask_order_id: OrderId,
    /// The fee rate of the taker.
    taker_fee_rate: Permill,
    /// The size of each tick.
    tick_size: K, 
    /// The minimum amount of the order.
    lot_size: K,
}

impl<K: CritbitTreeIndex, V: Clone + PartialOrd + Default> Pool<K, V> {
    pub fn new() -> Self {
        Self {
            bids: OrderBook::new(),
            asks: OrderBook::new(),
            ..Default::default()
        }
    }
}


