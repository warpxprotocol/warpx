# warp(x)

[**_warp(x)_**](https://warpx.trade) is a fully on-chain [hybrid orderbook](./pallets/hybrid-orderbook/README.md) exchange with AMM implemented in [Rust](https://www.rust-lang.org/)

# Features

> Those are the features implemented in _warp(x)_ runtime

## Hybrid Orderbook

Pool is always located at the middle of bid/ask orderbook where the current price of pair is made and eliminating the spread between those two. As a result, order is matched at the better price, which is taking instant arbitrage, the market provided.

**Process of matching order**

- When user place an order, order is matched on _pool_
- If the price(of the pool) reached the lowest ask or highest bid, order is matched on _orderbook_.
- This process repeats until the order is fully filled

**Reward Distribution**

- Takers Pay Makers Earn
- No matter how(e.g limit order or add liquidity to the pool) user provides its liquidity to the pool, will get rewards of trading fee

## NOMT

> [the Nearly-Optimal Merkle Trie Database](https://www.rob.tech/blog/introducing-nomt/)

- Storage layer
- Since fully on-chain matching order requires large amount of storage io, it is important to make cost efficient as much as possible. _warp(x)_ applied _NOMT_

## Multi Asset

- Since _warp(x)_ is based on _PolkadotSDK_, which is the best framework for composability, _XCM_ is supported natively. Other assets like DOT, EVM-compatible assets(e.g ETH, WBTC) are bridged via XCM.

## Pre-Confirmation

> Inspired by [Flashbots' Flashblocks](https://www.theblock.co/post/363109/coinbase-base-flashblocks) technology on Ethereum L2

- **Real-time Block Building**: _warp(x)_ implements a pre-confirmation system that provides real-time visibility into block construction through WebSocket subscriptions. Users can track their transactions being included in blocks before final confirmation.

- **Incremental Block Building**: Blocks are built incrementally as sub-blocks, allowing for faster transaction confirmation visibility. This approach significantly reduces the time users need to wait to know if their transactions will be included in the next block.

- **Instant Trade Visibility**: Unlike traditional blockchains where users must wait for block finalization, _warp(x)_'s pre-confirmation system enables near-instant feedback on transaction inclusion, improving the trading experience especially for time-sensitive operations.

**How it works**

- During block production, the proposer builds blocks incrementally as sub-blocks at regular intervals (configurable `preconf_block_time`)
- Each sub-block payload is broadcasted via WebSocket to subscribed clients in real-time
- Users can subscribe to the WebSocket endpoint to receive pre-confirmation updates about their transactions
- This allows applications and users to react to transaction inclusion much faster than waiting for final block confirmation

## Private Trading

- With the power of ZKP, _warp(x)_ enables users to trade without revealing their identity on-chain. That said, only the token and its amount of order is revealed
- This feature is still work in progress

# Milestones

### Phase 1

- [x] Pallet Hybrid Orderbook
- [x] Support `Limit Order` & `Market Order`
- [ ] Support `Stop-Limit Order` & `Stop Order`

### Phase 2

- [x] Pre-Confirmation
- [ ] Future Trading

### Phase 3

- [ ] Integrates multi-assets such as EVM-compatible or other parachains' assets

### Phase 4

- [ ] Private Trading(e.g Dark Pool)
- [ ] Integrates NOMT DB

# Why Polkadot?

### Polkadot offers several compelling advantages for our project:

1. **Robust Shared Security**: Polkadot's unique shared security model ensures that all parachains benefit from the collective security of the entire network. This approach significantly enhances the resilience and integrity of our chain, providing a solid foundation for our exchange.

2. **Cost-Effective Scalability**: By leveraging Polkadot's architecture(a.k.a [agile coretime](https://www.google.com/url?sa=t&source=web&rct=j&opi=89978449&url=https://polkadot.com/agile-coretime&ved=2ahUKEwjbqsK7_IOJAxXqs1YBHfGJGpEQFnoECBoQAQ&usg=AOvVaw1HBFk9EvvSMThQvXELFJTi) and [elastic scaling](https://www.google.com/url?sa=t&source=web&rct=j&opi=89978449&url=https://wiki.polkadot.network/docs/learn-elastic-scaling&ved=2ahUKEwis-rPR_IOJAxUa2TQHHUuUJfwQFnoECC8QAQ&usg=AOvVaw3kN8xpBhhVIga4m9Bq-pyw)), we can achieve high transaction throughput at lower costs compared to traditional roll-up solutions. This efficiency allows us to offer competitive fees while maintaining optimal performance which enables **fully-on-chain** trading.

3. **Composability**: Through its native Cross-Consensus Message (XCM) protocol, Polkadot enables seamless interoperability between different parachains and even external networks such as Ethereum and Solana. This composability is crucial for creating a truly interconnected and efficient ecosystem.

These features make Polkadot an ideal platform for building our hybrid orderbook exchange, enabling us to create a secure, scalable, and interconnected trading environment.

### What we want to achieve on Polkadot:

- **Fully On-Chain Trading**: Unlike other centralized exchanges, _warp(x)_ will not have any server or backend. Everything is executed on-chain, ensuring transparency and trustlessness.
- **Composability**: _warp(x)_ will be fully integrated into the Polkadot ecosystem, allowing for seamless interoperability with other parachains and even external networks.
- **Security**: With the shared security of Polkadot, _warp(x)_ will benefit from the security of the entire network, ensuring that the exchange remains secure even in the face of potential attacks.
- **High Liquidity**: _warp(x)_ will provide high liquidity for trading pairs, making it easier for users to buy and sell assets.
