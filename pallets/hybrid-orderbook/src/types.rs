
use super::*;
use super::critbit::CritbitTree;

/// Identifier for each order. Highest bit of the order id is used to indicate the order type (0 = bid, 1 = ask).
pub type OrderId = u64;
pub type Tick = u64;
// pub type OrderBook<T> = CritbitTree<>;
// pub type Order = Value;

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
pub struct Pool<Balance> {
    // Liquidity Pool
    base_amount: Balance,
    quote_amount: Balance,
    // Orderbook
    bids: BTreeMap<Tick, Vec<OrderId>>,
    asks: BTreeMap<Tick, Vec<OrderId>>,
    lot_size: Option<Tick>,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct Order<HigherPrecisionOrder, AccountId, BlockNumber> {
    price: HigherPrecisionOrder,
    quantity: HigherPrecisionOrder,
    owner: AccountId,
    expired_at: BlockNumber
}
