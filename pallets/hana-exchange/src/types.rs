
use super::*;
pub type MarketId = u32;

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct Market<AssetId, AccountId, BlockNumber, Amount, Price> {
    base_asset: AssetId,
    quote_asset: AssetId,
    price_decimals: u8,
    // pair
    base_amount: Amount,
    quote_amount: Amount,
    amount_shares: Amount,
    // Orderbook
    bid: Vec<Order<BlockNumber, AccountId, Price, Amount>>,
    ask: Vec<Order<BlockNumber, AccountId, Price, Amount>>,
    // History
    history: Vec<Tick<BlockNumber, Price, Amount>>
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct Order<BlockNumber, AccountId, Price, Amount> {
    price: Price,
    amount: Amount,
    owner: AccountId,
    timestamp: BlockNumber
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct Tick<BlockNumber, Price, Amount> {
    price: Price,
    amount: Amount,
    timestamp: BlockNumber
}