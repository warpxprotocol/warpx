# warp(x)

[**_warpX_**](https://warpx.vercel.app) is a fully on-chain [hybrid orderbook](./pallets/hybrid-orderbook/README.md) exchange with AMM implemented in [Rust](https://www.rust-lang.org/).

## Why Polkadot?

### Polkadot offers several compelling advantages for our project:

1. **Robust Shared Security**: Polkadot's unique shared security model ensures that all parachains benefit from the collective security of the entire network. This approach significantly enhances the resilience and integrity of our chain, providing a solid foundation for our exchange.

2. **Cost-Effective Scalability**: By leveraging Polkadot's architecture(a.k.a [agile coretime](https://www.google.com/url?sa=t&source=web&rct=j&opi=89978449&url=https://polkadot.com/agile-coretime&ved=2ahUKEwjbqsK7_IOJAxXqs1YBHfGJGpEQFnoECBoQAQ&usg=AOvVaw1HBFk9EvvSMThQvXELFJTi) and [elastic scaling](https://www.google.com/url?sa=t&source=web&rct=j&opi=89978449&url=https://wiki.polkadot.network/docs/learn-elastic-scaling&ved=2ahUKEwis-rPR_IOJAxUa2TQHHUuUJfwQFnoECC8QAQ&usg=AOvVaw3kN8xpBhhVIga4m9Bq-pyw)), we can achieve high transaction throughput at lower costs compared to traditional roll-up solutions. This efficiency allows us to offer competitive fees while maintaining optimal performance which enables **fully-on-chain** trading.

3. **Composability**: Through its native Cross-Consensus Message (XCM) protocol, Polkadot enables seamless interoperability between different parachains and even external networks such as Ethereum and Solana. This composability is crucial for creating a truly interconnected and efficient ecosystem.

These features make Polkadot an ideal platform for building our hybrid orderbook exchange, enabling us to create a secure, scalable, and interconnected trading environment.

### What we want to achieve on Polkadot:

- **Fully On-Chain Trading**: Unlike other centralized exchanges, _warpX_ will not have any server or backend. Everything is executed on-chain, ensuring transparency and trustlessness.
- **Composability**: _warpX_ will be fully integrated into the Polkadot ecosystem, allowing for seamless interoperability with other parachains and even external networks.
- **Security**: With the shared security of Polkadot, _warpX_ will benefit from the security of the entire network, ensuring that the exchange remains secure even in the face of potential attacks.
- **High Liquidity**: _warpX_ will provide high liquidity for trading pairs, making it easier for users to buy and sell assets.

# Features

> Those are the features implemented in _warpX_ runtime

## Hybrid Orderbook

Pool is always located at the middle of the bid/ask orderbook where the current price of the pair is determined, eliminating the spread between the two. As a result, orders are matched at a better price, which takes advantage of the instant arbitrage opportunity provided by the market.

**Process of matching order**

- When user place an order, order is matched on _pool_
- If the price(of the pool) reached the lowest ask or highest bid, order is matched on _orderbook_.
- This process repeats until the order is fully filled

**Reward Distribution**

- Takers Pay Makers Earn
- No matter how (e.g limit order or add liquidity to the pool) the user provide liquidity to the pool, they will receive rewards from trading fees.

## NOMT

> [the Nearly-Optimal Merkle Trie Database](https://www.rob.tech/blog/introducing-nomt/)

- Storage layer
- Since fully on-chain order matching requires a large amount of storage I/O, it is important to make it as cost-efficient as possible. _warpX_ applied _NOMT_.

## Multi Asset

- Since _warpX_ is based on _PolkadotSDK_, which is the best framework for composability, _XCM_ is supported natively. Other assets like DOT, EVM-compatible assets(e.g ETH, WBTC) are bridged via XCM.

## Private Trading

- With the power of ZKP, _warpX_ enables users to trade without revealing their identity on-chain. That said, only the token and its amount of order is revealed.
- This feature is still work in progress.

# Milestones

### Phase 1

- [x] Pallet Hybrid Orderbook
- [x] Support `Limit Order` & `Market Order`
- [ ] Support `Stop-Limit Order` & `Stop Order`

### Phase 2

- [ ] Integrate NOMT

### Phase 3

- [ ] Integrate multi-assets from other parachains or EVM-compatible blockchains

### Phase 4

- [ ] Private Trading
