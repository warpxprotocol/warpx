# Pallet Hybrid Orderbook

> Fully on-chain decentralized exchange combined traditional orderbook and liquidity pool(AMM)

## Overview

Traditional orderbook-based trading shows the psychology and intent of buyers and sellers in the market, as well as the level of liquidity. The orderbook operates in real-time, constantly updating to reflect the current state of the market. Order matching occurs by matching the lowest priced sell order with the highest priced buy order. The difference between the two prices is called _spread_, which indicates market depth and liquidity. For example, a tight spread generally signifies a liquid market, minimizing trading costs (a.k.a slippage) for market participants. In contrast, a wider spread represents an illiquid market with higher trading costs when trades occur. To provide constant liquidity and facilitate active trading, exchanges utilize _Market Makers_ (a.k.a MM).

Advantages of Ordebook:

- High capital efficiency
- Transparency: Get a transparent view of market supply and demand

On the other hand, AMM-based trading pools provide liquidity and **_automatically match orders_** based on the price set by the pool which basically has **_no spread_**.

Advantages of AMM:

- Automatically match orders with zero spread
- Market can be created permissonlessly
- LP get rewards

**_Hybrid Orderbook_** combines the best part of both worlds:

- Traditional orderbook model with an Automated Market Maker(a.k.a AMM) so that trading pairs with wide spreads still have automated market making, **creating an effect of zero spread**.
- There is a liquidity pool in the middle of the orderbook, and all orders (buys or sells) occur at the best price between the two. For example, a market buy order will take liquidity from the pool if cheaper than the orderbook, or vice versa. That said, orders will always be matched with the better price between the pool and orderbook.

### Order Types

- Market Order: Participants buy or sell immediately at the best available current market price.
- Limit Order: Participants set a specific price to buy or sell.
- Stop Limit Order: Triggers a limit order at a specified price (Y) when a certain price (X) is reached (e.g. stop loss).
- Stop Order: Triggers a market order at a defined offset (D) below the stop price (X) once price hits X.

Hybrid Orderbook combines the traditional orderbook model with an Automated Market Maker (a.k.a AMM) so that trading pairs with wide spreads still have automated market making, creating an effect of zero spread. There is a liquidity pool in the middle of the orderbook, and all orders (buys or sells) occur at the best price between the two. For example, a market buy order will take liquidity from the pool if cheaper than the orderbook, or vice versa.

### Fee Model

Market makers (limit orders or liquidity providers) earn compensation from taker fees (market orders).
`Maker Fee` + `Platform Fee` = `Taker Fee`

### Stop/Stop-Limit Order Types

User stop orders utilize a scheduler to automatically execute transactions at specified prices on the user's behalf.

## Goals

- Zero Spread
  Liquidity pools integrated into the orderbook eliminate spreads for seamless trading.

- Fully on-chain Exchange
  All info (e.g. pools, orderbooks) stored on-chain, and order matching occurs on-chain.

- Interoperable
  Bridges using light client proofs enable cross-chain trading between different consensus blockchains.

- Private Trading
  It may be possible to hide order amounts(e.g quantity) and who placed them(e.g orderer) with zero-knowledge proofs. Only the bid/ask events are visible on the explorer

**Two options for privacy:**

- Trade tokens with built-in zero-knowledge proofs (e.g. ZKERC20).
- Generate ZK proof for orders, hide order amounts.

## Details

### Parameters

**Markets**

Holds information about all available trading pairs.

```rust
  type Markets = Map<MarketId, Market>;
```

### Dispatchables

**create_pool(base_asset, quote_asset, taker_fee_rate, tick_size, lot_size)**

- _Creates a new tradeable pair._

**Notes:**

- `market_id` increments for each created pair
- Checks if account owns `asset_id`

**add_liquidity(base_asset, quote_asset)**

- _Adds liquidity to the pair associated with market_id. Earns LP tokens as reward._

**remove_liquidity(base_asset, quote_asset)**

- _Allows you to remove liquidity by providing the `lp_token` tokens that will be burned in the process._

**limit_order(base_asset, quote_asset, is_bid, price, quantity)**

- _Places an limit order. Order fills create Tick events stored in history._

**market_order(base_asset, quote_asset, quantity, is_bid)**

- Order matched based on _pool_ price until spread reaches zero between _bid_ and _ask_. After that, remain order quantity will be filled on order book. This process is repeated until order is fully filled.

**stop_order(base_asset, quote_asset)**

- Schedule order to be executed at a price that is a certain offset below the current market price.

**stop_limit_order(base_asset, quote_asset)**

- Schedule order to be executed at a price that is a certain offset below the current market price.

**cancel_order(base_asset, quote_asset, order_id)**

- Cancel order for given asset pair.

**Note:**

- signer must match order creator.

## Terminology

**Pool**

Holds trade data for a pair (e.g. `ETH <> USD`), including LiquidityPool and Orderbook info.

**Order**

Represents orderbook order data like size at price (e.g. amount_order) and filled amount (e.g. amount_order_dealt).

**Tick**

Captures trade details like when (e.g. BlockNumber), how much volume (e.g. Volume), price, and whether it came from the pool or orderbook.

## Types

```rust
// Generic type for orderbook which can be configured on Runtime
pub struct Pool<OrderBook> {
  /// Liquidity pool asset
	pub lp_token: AssetId,
  /// The orderbook of the bid.
  pub bids: OrderBook,
  /// The orderbook of the ask.
  pub asks: OrderBook,
  /// The next order id of the bid.
  pub next_bid_order_id: OrderId,
  /// The next order id of the ask.
  pub next_ask_order_id: OrderId,
  /// The fee rate of the taker.
  pub taker_fee_rate: Permill,
  /// The size of each tick.
  pub tick_size: Unit,
  /// The minimum amount of the order.
  pub lot_size: Unit,
}

pub struct Order<Quantity, Account, BlockNumber> {
    quantity: Quantity,
    owner: Account,
    expired_at: BlockNumber,
}

struct Liquidity<AssetId> {
	asset_id: AssetId,
}

pub struct Tick<Quantity, Account, BlockNumber> {
    next_order_id: OrderId,
    open_orders: BTreeMap<OrderId, Order<Quantity, Account, BlockNumber>>,
}
```

## OrderBook

### Critbit Tree

- Fully generic type for key/value
- Index of tree is partitioned based on `K::PARTITION_INDEX`
- If index is less than `K::PARTITION_INDEX`, it means _internal nodes_. Otherwise it means _leaf nodes_.
- `key` is `price` which would be based on tick size
- `value` is `Tick`

```rust
pub struct CritbitTree<K, V> {
    /// Index of the root node which is part of the internal nodes.
    root: K,
    /// The internal nodes of the tree.
    internal_nodes: BTreeMap<K, InternalNode<K>>,
    /// The leaf nodes of the tree.
    leaves: BTreeMap<K, LeafNode<K, V>>,
    /// Index of the largest value of the leaf nodes. Could be updated for every insertion.
    max_leaf_index: K,
    /// Index of the smallest value of the leaf nodes. Could be updated for every insertion.
    min_leaf_index: K,
    /// Index of the next internal node which should be incremented for every insertion.
    next_internal_node_index: K,
    /// Index of the next leaf node which should be incremented for every insertion.
    next_leaf_node_index: K,
}

pub struct LeafNode<K, V> {
    /// Parent index of the node.
    parent: K,
    /// Key of the node.
    key: K,
    /// Value of the node.
    value: V
}

pub struct InternalNode<K> {
    /// Mask for branching the tree based on the critbit.
    mask: K,
    /// Parent index of the node.
    parent: K,
    /// Left child index of the node.
    left: K,
    /// Right child index of the node.
    right: K,
}
```
