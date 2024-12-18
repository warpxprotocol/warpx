// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use core::{marker::PhantomData, ops::BitAnd};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_core::{RuntimeDebug, U256};
use sp_runtime::traits::{AtLeast32BitUnsigned, TryConvert};
use sp_std::{prelude::ToOwned, vec::Vec};

pub use traits::{OrderBook, OrderBookIndex};

pub type AssetIdOf<T> =
    <<T as Config>::Assets as Inspect<<T as frame_system::Config>::AccountId>>::AssetId;
pub type AssetBalanceOf<T> =
    <<T as Config>::Assets as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

impl<T: Config, AssetId, AccountId, Balance> FrozenBalance<AssetId, AccountId, Balance>
    for Pallet<T>
where
    AssetId: Into<T::AssetKind>,
    AccountId: Into<T::AccountId> + Clone,
    Balance: From<T::Unit>,
{
    fn frozen_balance(asset: AssetId, who: &AccountId) -> Option<Balance> {
        let _who: T::AccountId = who.clone().into();
        let _asset: T::AssetKind = asset.into();
        if let Some(frozen) = FrozenAssets::<T>::get(_who, _asset) {
            return Some(frozen.into());
        }
        None
    }

    fn died(_asset: AssetId, _who: &AccountId) {}
}

/// Order id of _hybrid orderbook_ which wrapped `u64`
#[derive(
    Decode,
    Encode,
    Debug,
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    MaxEncodedLen,
    TypeInfo,
)]
pub struct OrderId(pub u64);

#[derive(Debug)]
pub enum OrderIdError {
    Overflow,
    MaxIndex,
}

impl OrderbookOrderId for OrderId {
    type Error = OrderIdError;

    fn new(is_bid: bool) -> Self {
        if is_bid {
            Self(0)
        } else {
            Self(1 << (u64::BITS - 1))
        }
    }

    fn checked_increase(&self, is_bid: bool) -> Option<Self> {
        let max_index = if is_bid {
            1 << (u64::BITS - 1)
        } else {
            u64::MAX
        };

        let new = self.0.checked_add(1);
        match new {
            Some(new) => {
                // Over max index
                if new > max_index {
                    return None;
                }
                Some(new.into())
            }
            // Overflow
            None => None,
        }
    }

    fn is_bid(&self) -> bool {
        self.0 & (1 << (u64::BITS - 1)) == 0
    }
}

impl core::ops::Deref for OrderId {
    type Target = u64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::Add for OrderId {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Sub for OrderId {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl core::ops::AddAssign for OrderId {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl core::ops::SubAssign for OrderId {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl core::ops::Add<u64> for OrderId {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl core::ops::AddAssign<u64> for OrderId {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl From<u64> for OrderId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Into<u64> for OrderId {
    fn into(self) -> u64 {
        self.0
    }
}

/// Interface for `order_id` of orderbook
pub trait OrderbookOrderId:
    Sized
    + Copy
    + core::ops::Add
    + core::ops::Add<u64>
    + core::ops::Sub
    + core::ops::AddAssign
    + core::ops::AddAssign<u64>
    + core::ops::SubAssign
    + core::ops::Deref
{
    type Error;

    /// Create new instance of `OrderId`
    fn new(is_bid: bool) -> Self;
    /// Increase `OrderId` by 1. Return `None`, if overflow
    fn checked_increase(&self, is_bid: bool) -> Option<Self>;
    /// Check whether it is id of `bid` order
    fn is_bid(&self) -> bool;
}

/// Represents a swap path with associated asset amounts indicating how much of the asset needs to
/// be deposited to get the following asset's amount withdrawn (this is inclusive of fees).
///
/// Example:
/// Given path [(asset1, amount_in), (asset2, amount_out2), (asset3, amount_out3)], can be resolved:
/// 1. `asset(asset1, amount_in)` take from `user` and move to the pool(asset1, asset2);
/// 2. `asset(asset2, amount_out2)` transfer from pool(asset1, asset2) to pool(asset2, asset3);
/// 3. `asset(asset3, amount_out3)` move from pool(asset2, asset3) to `user`.
pub(super) type BalancePath<T> = Vec<(<T as Config>::AssetKind, <T as Config>::Unit)>;

/// Credit of [Config::Assets].
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Assets>;

/// Stores the lp_token asset id a particular pool has been assigned.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<PoolAssetId> {
    /// Liquidity pool asset
    pub lp_token: PoolAssetId,
}

/// Value of Orderbook. All orders for the `Tick` level stored.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, PartialOrd, RuntimeDebug, TypeInfo)]
pub struct Tick<Quantity, Account, BlockNumber> {
    /// All open orders for this `Tick` level. Stored on `BTreeMap`, where key is `OrderId` and the
    /// value is `Order`.
    open_orders: BTreeMap<OrderId, Order<Quantity, Account, BlockNumber>>,
}

impl<Quantity, Account: Clone, BlockNumber> Tick<Quantity, Account, BlockNumber> {
    /// Create new instance of `Tick`
    pub fn new(
        order_id: OrderId,
        owner: Account,
        quantity: Quantity,
        expired_at: BlockNumber,
    ) -> Self {
        let mut open_orders = BTreeMap::new();
        open_orders.insert(order_id, Order::new(owner, quantity, expired_at));
        Self { open_orders }
    }
}

/// The order of the orderbook.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, PartialOrd, RuntimeDebug, TypeInfo)]
pub struct Order<Quantity, Account, BlockNumber> {
    quantity: Quantity,
    owner: Account,
    expired_at: BlockNumber,
}

impl<Quantity, Account: Clone, BlockNumber> Order<Quantity, Account, BlockNumber> {
    pub fn new(owner: Account, quantity: Quantity, expired_at: BlockNumber) -> Self {
        Self {
            quantity,
            owner,
            expired_at,
        }
    }

    pub fn owner(&self) -> Account {
        self.owner.clone()
    }
}

/// Detail of the pool
#[derive(Encode, Decode, Default, Debug, Clone, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    /// Liquidity pool asset
    pub lp_token: T::PoolAssetId,
    /// The orderbook of the bid.
    pub bids: T::OrderBook,
    /// The orderbook of the ask.
    pub asks: T::OrderBook,
    /// The next order id of the bid. Starts from `0`d
    pub next_bid_order_id: OrderId,
    /// The next order id of the ask. Starts from (1 << BITS::63 - 1)
    pub next_ask_order_id: OrderId,
    /// The fee rate of the taker.
    pub taker_fee_rate: Permill,
    /// The size of each tick.
    pub tick_size: T::Unit,
    /// The minimum amount of the order.
    pub lot_size: T::Unit,
}

impl<T: Config> Pool<T> {
    /// Create new instance of Pool
    pub fn new(
        lp_token: T::PoolAssetId,
        taker_fee_rate: Permill,
        tick_size: T::Unit,
        lot_size: T::Unit,
    ) -> Self {
        Self {
            lp_token,
            bids: T::OrderBook::new(),
            asks: T::OrderBook::new(),
            next_bid_order_id: OrderId::new(true),
            next_ask_order_id: OrderId::new(false),
            taker_fee_rate,
            tick_size,
            lot_size,
        }
    }

    pub fn lp_token(&self) -> T::PoolAssetId {
        self.lp_token.clone()
    }

    // test only
    pub fn orders_for(
        &self,
        owner: &T::AccountId,
        is_bid: bool,
    ) -> Vec<Order<T::Unit, T::AccountId, BlockNumberFor<T>>> {
        if is_bid {
            self.bids.get_orders(owner)
        } else {
            self.asks.get_orders(owner)
        }
    }

    pub fn next_bid_order_id(&mut self) -> Result<OrderId, Error<T>> {
        let next = self.next_bid_order_id.clone();
        self.next_bid_order_id = next.checked_increase(true).ok_or(Error::<T>::Overflow)?;
        Ok(next)
    }

    pub fn next_ask_order_id(&mut self) -> Result<OrderId, Error<T>> {
        let next = self.next_ask_order_id.clone();
        self.next_ask_order_id = next.checked_increase(false).ok_or(Error::<T>::Overflow)?;
        Ok(next)
    }

    /// Get the clone of the bid orders from orderbook
    pub fn bid_orders(&self) -> T::OrderBook {
        self.bids.clone()
    }

    /// Get the clone of the ask orders from orderbook
    pub fn ask_orders(&self) -> T::OrderBook {
        self.asks.clone()
    }

    /// Get the next bid order that should be filled. Return `Some((price, index))` if any.
    /// Otherwise, return `None` which means there's no `bid` orders on the Orderbook
    pub fn next_bid_order(&self) -> Option<(T::Unit, T::Unit)> {
        self.bids.max_order()
    }

    /// Get the next ask order that should be filled. Return `Some((price, index))` if any.
    /// Otherwise, return `None` which means there's no `ask` orders on the Orderbook
    pub fn next_ask_order(&self) -> Option<(T::Unit, T::Unit)> {
        self.asks.min_order()
    }

    /// Place `quantity` amount of orders of give `price`. This method will be only called when
    /// `do_limit_order`. `OrderId` will be returned if success.
    pub fn place_order(
        &mut self,
        is_bid: bool,
        owner: &T::AccountId,
        price: T::Unit,
        quantity: T::Unit,
    ) -> Result<(T::Unit, OrderId), Error<T>> {
        let current = frame_system::Pallet::<T>::block_number();
        let expired_at = current + T::OrderExpiration::get();
        let order_id: OrderId;
        if is_bid {
            order_id = self.next_bid_order_id()?;
            self.bids
                .place_order(order_id, &owner, price, quantity, expired_at)
                .map_err(|_| Error::<T>::ErrorOnPlaceOrder)?;
        } else {
            order_id = self.next_ask_order_id()?;
            self.asks
                .place_order(order_id, &owner, price, quantity, expired_at)
                .map_err(|_| Error::<T>::ErrorOnPlaceOrder)?;
        };

        Ok((price, order_id))
    }

    /// Fill `quantity` amount of orders of given `price`.
    pub fn fill_order(
        &mut self,
        is_bid: bool,
        price: T::Unit,
        quantity: T::Unit,
    ) -> Result<Option<Vec<(T::AccountId, T::Unit)>>, Error<T>> {
        let res = if is_bid {
            self.asks.fill_order(price, quantity)
        } else {
            self.bids.fill_order(price, quantity)
        };

        res.map_err(|_| Error::<T>::ErrorOnFillOrder)
    }

    /// Cancel order of `owner`
    pub fn cancel_order(
        &mut self,
        maybe_owner: &T::AccountId,
        price: T::Unit,
        order_id: OrderId,
        quantity: T::Unit,
    ) -> Result<OrderId, Error<T>> {
        if order_id.is_bid() {
            self.bids
                .cancel_order(maybe_owner, price, order_id, quantity)
                .map_err(|_| Error::<T>::ErrorOnCancelOrder)?;
        } else {
            self.asks
                .cancel_order(maybe_owner, price, order_id, quantity)
                .map_err(|_| Error::<T>::ErrorOnCancelOrder)?;
        }
        Ok(order_id)
    }
}

/// Provides means to resolve the `PoolId` and `AccountId` from a pair of assets.
///
/// Resulting `PoolId` remains consistent whether the asset pair is presented as (asset1, asset2)
/// or (asset2, asset1). The derived `AccountId` may serve as an address for liquidity provider
/// tokens.
pub trait PoolLocator<AccountId, AssetKind, PoolId> {
    /// Retrieves the account address associated with a valid `PoolId`.
    fn address(id: &PoolId) -> Result<AccountId, ()>;
    /// Identifies the `PoolId` for a given pair of assets.
    ///
    /// Returns an error if the asset pair isn't supported.
    fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<PoolId, ()>;
    /// Retrieves the account address associated with a given asset pair.
    ///
    /// Returns an error if the asset pair isn't supported.
    fn pool_address(asset1: &AssetKind, asset2: &AssetKind) -> Result<AccountId, ()> {
        if let Ok(id) = Self::pool_id(asset1, asset2) {
            Self::address(&id)
        } else {
            Err(())
        }
    }
}

/// Pool locator that mandates the base asset and quote asset are different.
///
/// The `PoolId` is represented as a tuple of `AssetKind`s with `BaseAsset` always positioned as
/// the first element.
pub struct BaseQuoteAsset<AccountId, AssetKind>(PhantomData<(AccountId, AssetKind)>);
impl<AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
    for BaseQuoteAsset<AccountId, AssetKind>
where
    AssetKind: Eq + Clone + Encode,
    AccountId: Decode,
{
    fn pool_id(
        base_asset: &AssetKind,
        quote_asset: &AssetKind,
    ) -> Result<(AssetKind, AssetKind), ()> {
        if base_asset == quote_asset {
            return Err(());
        }
        Ok((base_asset.clone(), quote_asset.clone()))
    }

    fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
        let encoded = sp_io::hashing::blake2_256(&Encode::encode(id)[..]);
        Decode::decode(&mut TrailingZeroInput::new(encoded.as_ref())).map_err(|_| ())
    }
}

/// Pool locator that mandates the inclusion of the specified `FirstAsset` in every asset pair.
///
/// The `PoolId` is represented as a tuple of `AssetKind`s with `FirstAsset` always positioned as
/// the first element.
pub struct WithFirstAsset<FirstAsset, AccountId, AssetKind>(
    PhantomData<(FirstAsset, AccountId, AssetKind)>,
);
impl<FirstAsset, AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
    for WithFirstAsset<FirstAsset, AccountId, AssetKind>
where
    AssetKind: Eq + Clone + Encode,
    AccountId: Decode,
    FirstAsset: Get<AssetKind>,
{
    fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
        let first = FirstAsset::get();
        match true {
            _ if asset1 == asset2 => Err(()),
            _ if first == *asset1 => Ok((first, asset2.clone())),
            _ if first == *asset2 => Ok((first, asset1.clone())),
            _ => Err(()),
        }
    }
    fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
        let encoded = sp_io::hashing::blake2_256(&Encode::encode(id)[..]);
        Decode::decode(&mut TrailingZeroInput::new(encoded.as_ref())).map_err(|_| ())
    }
}

/// Pool locator where the `PoolId` is a tuple of `AssetKind`s arranged in ascending order.
pub struct Ascending<AccountId, AssetKind>(PhantomData<(AccountId, AssetKind)>);
impl<AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
    for Ascending<AccountId, AssetKind>
where
    AssetKind: Ord + Clone + Encode,
    AccountId: Decode,
{
    fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
        match true {
            _ if asset1 > asset2 => Ok((asset2.clone(), asset1.clone())),
            _ if asset1 < asset2 => Ok((asset1.clone(), asset2.clone())),
            _ => Err(()),
        }
    }
    fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
        let encoded = sp_io::hashing::blake2_256(&Encode::encode(id)[..]);
        Decode::decode(&mut TrailingZeroInput::new(encoded.as_ref())).map_err(|_| ())
    }
}

/// Pool locator that chains the `First` and `Second` implementations of [`PoolLocator`].
///
/// If the `First` implementation fails, it falls back to the `Second`.
pub struct Chain<First, Second>(PhantomData<(First, Second)>);
impl<First, Second, AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
    for Chain<First, Second>
where
    First: PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>,
    Second: PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>,
{
    fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
        First::pool_id(asset1, asset2).or(Second::pool_id(asset1, asset2))
    }
    fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
        First::address(id).or(Second::address(id))
    }
}

/// `PoolId` to `AccountId` conversion.
pub struct AccountIdConverter<Seed, PoolId>(PhantomData<(Seed, PoolId)>);
impl<Seed, PoolId, AccountId> TryConvert<&PoolId, AccountId> for AccountIdConverter<Seed, PoolId>
where
    PoolId: Encode,
    AccountId: Decode,
    Seed: Get<PalletId>,
{
    fn try_convert(id: &PoolId) -> Result<AccountId, &PoolId> {
        sp_io::hashing::blake2_256(&Encode::encode(&(Seed::get(), id))[..])
            .using_encoded(|e| Decode::decode(&mut TrailingZeroInput::new(e)).map_err(|_| id))
    }
}

/// `PoolId` to `AccountId` conversion without an addition arguments to the seed.
pub struct AccountIdConverterNoSeed<PoolId>(PhantomData<PoolId>);
impl<PoolId, AccountId> TryConvert<&PoolId, AccountId> for AccountIdConverterNoSeed<PoolId>
where
    PoolId: Encode,
    AccountId: Decode,
{
    fn try_convert(id: &PoolId) -> Result<AccountId, &PoolId> {
        sp_io::hashing::blake2_256(&Encode::encode(id)[..])
            .using_encoded(|e| Decode::decode(&mut TrailingZeroInput::new(e)).map_err(|_| id))
    }
}

pub mod traits {

    use super::*;

    pub trait OrderBook<Account, Unit, BlockNumber> {
        /// Type of order used for value in orderbook
        type Order: OrderInterface<Account, Unit, BlockNumber>;
        /// Identifier for each order
        type OrderId: OrderbookOrderId;
        /// Type of error of orderbook
        type Error;

        /// Create new instance
        fn new() -> Self;

        /// Check if the orderbook is empty
        fn is_empty(&self) -> bool;

        fn get_orders(&self, owner: &Account) -> Vec<Order<Unit, Account, BlockNumber>>;

        fn open_orders_at(&self, key: Unit) -> Result<Option<Self::Order>, Self::Error>;

        /// Optionally return the minimum order of the orderbook which is (price, index).
        /// Return `None`, if the orderbook is empty
        fn min_order(&self) -> Option<(Unit, Unit)>;

        /// Optionally return the maximum order of the orderbook which is (price, index).
        /// Return `None`, if the orderbook is empty
        fn max_order(&self) -> Option<(Unit, Unit)>;

        /// Place a new order only used for limit order
        fn place_order(
            &mut self,
            order_id: Self::OrderId,
            owner: &Account,
            key: Unit,
            quantity: Unit,
            expired_at: BlockNumber,
        ) -> Result<(), Self::Error>;

        /// Fill order on orderbook used for order matching. Return (Account, Unit) which means
        /// orderer and the amount of filled orders
        fn fill_order(
            &mut self,
            key: Unit,
            quantity: Unit,
        ) -> Result<Option<Vec<(Account, Unit)>>, Self::Error>;

        /// Cancel the order
        fn cancel_order(
            &mut self,
            maybe_owner: &Account,
            key: Unit,
            order_id: Self::OrderId,
            quantity: Unit,
        ) -> Result<(), Self::Error>;
    }

    /// Index trait for the critbit tree.
    pub trait OrderBookIndex:
        sp_std::fmt::Debug + Default + AtLeast32BitUnsigned + Copy + BitAnd<Output = Self>
    {
        /// Maximum index value.
        const MAX_INDEX: Self;
        /// Partition index value. This index is for partitioning between internal and leaf nodes.
        /// Index of the internal nodes is always less than `PARTITION_INDEX`.
        /// While index of the leaf nodes is always greater than or equal to `PARTITION_INDEX`.
        const PARTITION_INDEX: Self;
        /// Maximum number of leaf nodes that can be stored in the tree.
        const CAPACITY: Self;

        fn is_partition_index(&self) -> bool {
            (*self) == Self::PARTITION_INDEX
        }

        /// Calculate new mask.
        /// First, find the position(pos) of the most significant bit in the XOR of the two indexes.
        /// Then, right shift the mask by that position(e.g. 1 << pos).
        fn new_mask(&self, closest_key: &Self) -> Self;
    }

    macro_rules! impl_orderbook_index {
        ($type: ty, $higher_type: ty) => {
            impl OrderBookIndex for $type {
                const MAX_INDEX: Self = <$type>::MAX;
                const PARTITION_INDEX: Self = 1 << (<$type>::BITS - 1);
                const CAPACITY: Self = <$type>::MAX_INDEX - <$type>::PARTITION_INDEX;

                fn new_mask(&self, closest_key: &Self) -> Self {
                    let critbit = <$higher_type>::from(self ^ closest_key);
                    let pos = <$type>::BITS - (critbit.leading_zeros() - <$type>::BITS);
                    1 << (pos - 1)
                }
            }
        };
    }

    impl_orderbook_index!(u32, u64);
    impl_orderbook_index!(u64, u128);
    impl_orderbook_index!(u128, U256);

    /// Abstraction for type of order should implement
    pub trait OrderInterface<Account, Unit, BlockNumber> {
        /// Type of order id(e.g `u64`)
        type OrderId: OrderbookOrderId;
        /// Type of order error
        type Error;

        /// Create new instance of `Order`
        fn new(
            order_id: Self::OrderId,
            owner: Account,
            quantity: Unit,
            expired_at: BlockNumber,
        ) -> Self;

        fn find_order_of(&self, owner: &Account) -> Option<Vec<Order<Unit, Account, BlockNumber>>>;

        fn orders(&self) -> Self;

        /// Return `true` if there are no open orders
        fn is_empty(&self) -> bool;

        /// Create new instance of order and return `OrderId` of that order.
        fn placed(
            &mut self,
            order_id: Self::OrderId,
            owner: &Account,
            quantity: Unit,
            expired_at: BlockNumber,
        );

        /// Fill the order with the given `quantity`
        fn filled(&mut self, quantity: Unit) -> Option<Vec<(Account, Unit)>>;

        /// Add the quantity to the order of the given order id
        fn added(
            &mut self,
            maybe_owner: &Account,
            order_id: Self::OrderId,
            quantity: Unit,
        ) -> Result<(), Self::Error>;

        /// Remove the `quantity` of order of the given order id
        fn canceled(
            &mut self,
            maybe_owner: &Account,
            order_id: Self::OrderId,
            quantity: Unit,
        ) -> Result<(), Self::Error>;
    }

    /// Error type for order
    #[derive(Debug)]
    pub enum OrderError {
        /// Order not found
        OrderNotExist,
        /// Order owner mismatch
        NotOwner,
        /// Quantity underflow
        Underflow,
        /// Quantity or OrderId might overflow
        Overflow,
    }

    impl<Unit, Account, BlockNumber> OrderInterface<Account, Unit, BlockNumber>
        for Tick<Unit, Account, BlockNumber>
    where
        Account: PartialEq + Clone,
        Unit: AtLeast32BitUnsigned + Copy,
        BlockNumber: Clone,
    {
        type OrderId = OrderId;
        type Error = OrderError;

        fn new(
            order_id: Self::OrderId,
            owner: Account,
            quantity: Unit,
            expired_at: BlockNumber,
        ) -> Self {
            Self::new(order_id, owner, quantity, expired_at)
        }

        // for test only
        fn find_order_of(&self, owner: &Account) -> Option<Vec<Order<Unit, Account, BlockNumber>>> {
            let matched = self
                .to_owned()
                .open_orders
                .into_values()
                .filter(|o| o.owner() == *owner)
                .collect::<Vec<Order<Unit, Account, BlockNumber>>>();
            if matched.is_empty() {
                return None;
            } else {
                Some(matched)
            }
        }

        fn orders(&self) -> Self {
            self.clone()
        }

        fn is_empty(&self) -> bool {
            self.open_orders.is_empty()
        }

        fn placed(
            &mut self,
            order_id: Self::OrderId,
            owner: &Account,
            quantity: Unit,
            expired_at: BlockNumber,
        ) {
            self.open_orders
                .insert(order_id, Order::new(owner.clone(), quantity, expired_at));
        }

        fn filled(&mut self, quantity: Unit) -> Option<Vec<(Account, Unit)>> {
            let mut filled = Zero::zero();
            let mut to_remove = Vec::new();
            let mut res: Vec<(Account, Unit)> = Vec::new();
            for (id, order) in self.open_orders.iter_mut() {
                if_std! {
                println!("👀 OrderId => {:?} is currently filled.", id);
                    }
                // All orders are filled
                let remain = quantity - filled;
                if remain == Zero::zero() {
                    break;
                }
                if order.quantity >= remain {
                    order.quantity -= remain;
                    filled += remain;
                    res.push((order.owner(), remain));
                } else {
                    filled += order.quantity;
                    // Order of `id` is fully filled and should be removed
                    to_remove.push(*id);
                    res.push((order.owner(), order.quantity));
                }
            }
            // Remove the fully filled orders
            for id in to_remove {
                self.open_orders.remove(&id);
            }

            if filled == Zero::zero() {
                None
            } else {
                Some(res)
            }
        }

        fn added(
            &mut self,
            maybe_owner: &Account,
            order_id: Self::OrderId,
            quantity: Unit,
        ) -> Result<(), Self::Error> {
            if let Some(order) = self.open_orders.get_mut(&order_id) {
                ensure!(order.owner == *maybe_owner, OrderError::NotOwner);
                order.quantity = order
                    .quantity
                    .checked_add(&quantity)
                    .ok_or(OrderError::Overflow)?;
                Ok(())
            } else {
                return Err(OrderError::OrderNotExist);
            }
        }

        fn canceled(
            &mut self,
            maybe_owner: &Account,
            order_id: Self::OrderId,
            quantity: Unit,
        ) -> Result<(), Self::Error> {
            if let Some(order) = self.open_orders.get_mut(&order_id) {
                ensure!(order.owner == *maybe_owner, OrderError::NotOwner);
                order.quantity = order
                    .quantity
                    .checked_sub(&quantity)
                    .ok_or(OrderError::Underflow)?;
                if order.quantity == Zero::zero() {
                    self.open_orders.remove(&order_id);
                }
                Ok(())
            } else {
                return Err(OrderError::OrderNotExist);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OrderBookIndex;

    #[test]
    fn partition_index_works() {
        let partition_index: u64 = 1 << (u64::BITS - 1);
        assert!(partition_index.is_partition_index());
        let not_partition_index: u64 = 1;
        assert!(!not_partition_index.is_partition_index());
    }

    #[test]
    fn new_mask_works() {}
}
