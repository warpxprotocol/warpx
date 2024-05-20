
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
pub struct Pool<Price, Amount, Account, BlockNumber> {
    /// The amount of the base asset.
    base_amount: Amount,
    /// The amount of the quote asset.
    quote_amount: Amount,
    /// The orderbook of the bid.
    bids: OrderBook<Price, Orders<Amount, Account, BlockNumber>>,
    /// The orderbook of the ask.
    asks: OrderBook<Price, Orders<Amount, Account, BlockNumber>>,
    // Orderbook
    next_bid_order_id: OrderId,
    /// The next order id of the ask.
    next_ask_order_id: OrderId,
    /// The fee rate of the taker.
    taker_fee_rate: Permill,
    /// The size of each tick.
    tick_size: Amount, 
    /// The minimum amount of the order.
    lot_size: Amount,
}
