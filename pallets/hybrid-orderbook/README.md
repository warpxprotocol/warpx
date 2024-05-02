# Pallet Hana Exchange

> Full-onchain decentralized exchange that combined orderbook and liquidity pool

## Overview

Traditional orderbook-based trading shows the psychology and intent of buyers and sellers in the market, as well as the level of liquidity. The orderbook operates in real-time, constantly updating to reflect the current state of the market, allowing market participants to gauge market sentiment and trends through orderbook data. Order matching occurs by matching the lowest priced sell order with the highest priced buy order. The difference between the two prices is called the spread, which indicates market depth and liquidity. For example, a tight spread generally signifies a liquid market, minimizing trading costs (e.g. slippage) for market participants. In contrast, a wider spread represents an illiquid market with higher trading costs when trades occur. To provide constant liquidity and facilitate active trading, exchanges utilize Market Makers (a.k.a MM).

### Orderbook Trade Types

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

**create_pair(asset_id, asset_id)**

_Creates a new tradeable pair._

**Notes:**

- `market_id` increments for each created pair
- Checks if account owns `asset_id`

**add_liquidity(market_id, liquidity)**

_Adds liquidity to the pair associated with market_id. Earns LP tokens as reward._

**order(market_id, order_type)**

_Places an order of order_type for the market market_id. Order fills create Tick events stored in history._

- `OrderType::Market`

Fills from best price between pool or orderbook.

- `OrderType::Limit(price)`

Places order at given price.

- `OrderType::Stop { at: Price, delta: Permil, limit: Price }`

  - When price reaches at (above current price), places order at limit offset by `delta(%)` from highest price.
  - Triggers `Call::order(market_id, OrderType::Limit)` when conditions met.

- `OrderType::StopLimit { below: Price, limit: Price }`

  - Places limit order at limit when price reaches below.
  - Triggers `Call::order(market_id, OrderType::Limit)` when price hit.

**cancel(market_id)**

_Cancels order for market_id_

**Note:**

- signer must match order creator.

## Terminology

**Market**

Holds trade data for a pair (e.g. `ETH <> USD`), including LiquidityPool and Orderbook info. Stores all trades.

**Order**

Represents orderbook order data like size at price (e.g. amount_order) and filled amount (e.g. amount_order_dealt).

**Tick**

Captures trade details like when (e.g. BlockNumber), how much volume (e.g. Volume), price, and whether it came from the pool or orderbook.

## Types

```rust
struct Market {
    base_asset: AssetId,
    quote_asset: AssetId,
    price_decimals: u8,
    // pair
    base_amount: u64,
    quote_amount: u64
    amount_shares: u64,
    // Orderbook
    bid: Vec<Order>,
    ask: Vec<Order>,
    // History
    history: Vec<Tick>
}

struct Order<BlockNumber, Account> {
    owner: Account
    when: BlockNumber
    // Amount of this order opens
    amount_order_open: Balance,
    // Amount of this order has been dealt
    amount_order_dealt: Balance,
    price: Balance,
}

struct Liquidity<AssetId, Balance> {
	asset_id: AssetId,
	amount: Balance
}

struct Tick<BlockNumber, Balance> {
    when: BlockNumber,
    volume: Balance,
    price: Balance,
    order_type: u8
}

pub enum OrderType<Price> {
	Market,
	Limit(price),
	Stop { at: Price, delta: Permil, limit: Price }
	StopLimit { below: Price, limit: Price}
}
```
