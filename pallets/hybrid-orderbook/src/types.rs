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
use core::marker::PhantomData;
use scale_info::TypeInfo;
use sp_runtime::traits::{TryConvert, AtLeast32BitUnsigned};
use sp_core::{RuntimeDebug, U256};
use sp_std::vec::Vec;
use core::ops::BitAnd;

pub use traits::{OrderBook, OrderBookIndex};

/// Identifier for each order
pub type OrderId = u64;

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

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, PartialOrd, RuntimeDebug, TypeInfo)]
pub struct Tick<Quantity, Account, BlockNumber> {
    next_order_id: OrderId,
    open_orders: BTreeMap<OrderId, Order<Quantity, Account, BlockNumber>>,
}

impl<Quantity, Account, BlockNumber> Tick<Quantity, Account, BlockNumber> {
    pub fn new(owner: Account, quantity: Quantity, expired_at: BlockNumber) -> Self {
        let mut order_id = 0;
        let mut open_orders = BTreeMap::new();
        open_orders.insert(order_id, Order::new(owner, quantity, expired_at));
        order_id += 1;
        Self {
            next_order_id: order_id,
            open_orders
        }
    }
}

/// The order of the orderbook.
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, PartialOrd, RuntimeDebug, TypeInfo)]
pub struct Order<Quantity, Account, BlockNumber> {
    quantity: Quantity,
    owner: Account,
    expired_at: BlockNumber,
}

impl<Quantity, Account, BlockNumber> Order<Quantity, Account, BlockNumber> {
    pub fn new(owner: Account, quantity: Quantity, expired_at: BlockNumber) -> Self {
        Self {
            quantity,
            owner,
            expired_at,
        }
    }
}

/// Detail of the pool 
#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    /// Liquidity pool asset
	pub lp_token: T::PoolAssetId,
    /// The orderbook of the bid.
    pub bids: T::OrderBook,
    /// The orderbook of the ask.
    pub asks: T::OrderBook,
    /// The next order id of the bid.
    pub next_bid_order_id: OrderId,
    /// The next order id of the ask.
    pub next_ask_order_id: OrderId,
    /// The fee rate of the taker.
    pub taker_fee_rate: Permill,
    /// The size of each tick.
    pub tick_size: T::Unit, 
    /// The minimum amount of the order.
    pub lot_size: T::Unit,
}

impl<T: Config> Pool<T> {
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
            next_bid_order_id: 0,
            next_ask_order_id: 0,
            taker_fee_rate,
            tick_size,
            lot_size,
        }
    }

    pub fn bid_orders(&self) -> T::OrderBook {
        self.bids.clone()
    }

    pub fn ask_orders(&self) -> T::OrderBook {
        self.asks.clone()
    }

    pub fn next_bid_order(&self) -> Option<(T::Unit, T::Unit)> {
        self.bids.max_order()
    }

    pub fn next_ask_order(&self) -> Option<(T::Unit, T::Unit)> {
        self.asks.min_order()
    }

    pub fn place_order(&mut self, is_bid: bool, owner: T::AccountId, price: T::Unit, quantity: T::Unit) -> Result<OrderId, Error<T>> {
        let current = frame_system::Pallet::<T>::block_number();
        let expired_at = current + T::OrderExpiration::get();
        let order_id = if is_bid {
            self.bids.place_order(owner, price, quantity, expired_at).map_err(|_| Error::<T>::ErrorOnPlaceOrder)?
        } else {
            self.asks.place_order(owner, price, quantity, expired_at).map_err(|_| Error::<T>::ErrorOnPlaceOrder)?
        };
        
        Ok(order_id.into())
    }

    pub fn fill_order(&mut self, is_bid: bool, price: T::Unit, quantity: T::Unit) -> Result<Option<T::Unit>, Error<T>> {
        let res = if is_bid {
            self.asks.fill_order(price, quantity)
        } else {
            self.bids.fill_order(price, quantity)
        };

        res.map_err(|_| Error::<T>::ErrorOnFillOrder)
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
impl<AccountId, AssetKind> PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)> for BaseQuoteAsset<AccountId, AssetKind> 
where
    AssetKind: Eq + Clone + Encode,
    AccountId: Decode,
{  
    fn pool_id(base_asset: &AssetKind, quote_asset: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
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
        type OrderId: AtLeast32BitUnsigned;
        /// Type of error of orderbook
        type Error;

        /// Create new instance
        fn new() -> Self;

        /// Create new order
        fn new_order(&mut self, key: Unit, order: Self::Order) -> Result<(), Self::Error>;

        /// Remove order
        fn remove_order(&mut self, key: Unit, order_id: Self::OrderId) -> Result<(), Self::Error>;

        /// Optionally return the minimum order of the orderbook which is (price, index).
        /// Return `None`, if the orderbook is empty
        fn min_order(&self) -> Option<(Unit, Unit)>;

        /// Optionally return the maximum order of the orderbook which is (price, index).
        /// Return `None`, if the orderbook is empty
        fn max_order(&self) -> Option<(Unit, Unit)>;

        /// Check if the orderbook is empty
        fn is_empty(&self) -> bool;

        /// Place a new order only used for limit order
        fn place_order(&mut self, owner: Account, key: Unit, quantity: Unit, expired_at: BlockNumber) -> Result<Self::OrderId, Self::Error>;

        /// Fill order on orderbook used for order matching
        fn fill_order(&mut self, key: Unit, quantity: Unit) -> Result<Option<Unit>, Self::Error>;
    }

	/// Index trait for the critbit tree.
    pub trait OrderBookIndex: sp_std::fmt::Debug + Default + AtLeast32BitUnsigned + Copy + BitAnd<Output=Self> {
        /// Maximum index value.
        const MAX_INDEX: Self;
        /// Partition index value. This index is for partitioning between internal and leaf nodes.
        /// Index of the internal nodes is always less than `PARTITION_INDEX`.
        /// While index of the leaf nodes is always greater than or equal to `PARTITION_INDEX`.
        const PARTITION_INDEX: Self;
        /// Maximum number of leaf nodes that can be stored in the tree.
        const CAPACITY: Self;

        /// Calculate new mask. 
        /// First, find the position(pos) of the most significant bit in the XOR of the two indexes.
        /// Then, right shift the mask by that position(e.g. 1 << pos).
        fn new_mask(&self, closest_key: &Self) -> Self;
    }

    macro_rules! impl_order_book_index {
        ($type: ty, $higher_type: ty) => {
            impl OrderBookIndex for $type {
                const MAX_INDEX: Self = <$type>::MAX;
                const PARTITION_INDEX: Self = 1 << (<$type>::BITS - 1);
                const CAPACITY: Self = <$type>::MAX_INDEX - <$type>::PARTITION_INDEX;
    
                fn new_mask(&self, closest_key: &Self) -> Self {
                    let critbit = <$higher_type>::from(self ^ closest_key);
                    let pos = <$type>::BITS - (critbit.leading_zeros() - <$type>::BITS);
                    1 << (pos-1)
                }
            }
        }
    }
    
    impl_order_book_index!(u32, u64);
    impl_order_book_index!(u64, u128);
    impl_order_book_index!(u128, U256);

    /// Abstraction for type of order should implement
    pub trait OrderInterface<Account, Unit, BlockNumber> {
        /// Type of order id(e.g `u64`)
        type OrderId: AtLeast32BitUnsigned;
        /// Type of order error
        type Error;

        fn new_order(owner: Account, quantity: Unit, expired_at: BlockNumber) -> Self;

        /// Create new instance of order
        fn place_order(&mut self, owner: Account, quantity: Unit, expired_at: BlockNumber) -> Self::OrderId;

        /// Fill the order with the given quantity
        fn fill_order(&mut self, quantity: Unit) -> Option<Unit>;

        /// Add the quantity to the order of the given order id
        fn add_order(&mut self, maybe_owner: &Account, order_id: Self::OrderId, quantity: Unit) -> Result<(), Self::Error>;

        /// Remove the quantity from the order of the given order id
        fn remove_order(&mut self, maybe_owner: &Account, order_id: Self::OrderId, quantity: Unit) -> Result<(), Self::Error>;
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
        /// Quantity overflow
        Overflow,
    }

    impl<Unit, Account, BlockNumber> OrderInterface<Account, Unit, BlockNumber> for Tick<Unit, Account, BlockNumber> 
    where
        Account: PartialEq,
        Unit: AtLeast32BitUnsigned + Copy
    {
        type OrderId = OrderId;
        type Error = OrderError;

        fn new_order(owner: Account, quantity: Unit, expired_at: BlockNumber) -> Self {
            Self::new(owner, quantity, expired_at)
        }

        fn place_order(&mut self, owner: Account, quantity: Unit, expired_at: BlockNumber) -> Self::OrderId {
            let order_id = self.next_order_id;
            self.next_order_id += 1;
            self.open_orders.insert(order_id, Order::new(owner, quantity, expired_at));
            order_id
        }

        fn fill_order(&mut self, quantity: Unit) -> Option<Unit> {
            let mut filled = quantity;
            let mut to_remove = Vec::new();
            for (id, order) in self.open_orders.iter_mut() {
                // All orders are filled
                if filled == Zero::zero() {
                    break;
                }
                if order.quantity >= quantity {
                    order.quantity -= quantity;
                    filled = Zero::zero();
                } else {
                    filled -= order.quantity;
                    // Order of `id` is fully filled and should be removed
                    to_remove.push(*id);
                }
            }
            // Remove the fully filled orders
            for id in to_remove {
                self.open_orders.remove(&id);
            }
            // If all orders are fully filled, call `done_fill()` and return `None`
            // Otherwise, return the remaining orders 
            if filled == Zero::zero() {
                None
            } else {
                Some(filled)
            }
        }

        fn add_order(&mut self, maybe_owner: &Account, order_id: Self::OrderId, quantity: Unit) -> Result<(), Self::Error> {
            if let Some(order) = self.open_orders.get_mut(&order_id) {
                ensure!(order.owner == *maybe_owner, OrderError::NotOwner);
                order.quantity = order.quantity.checked_add(&quantity).ok_or(OrderError::Overflow)?;
                Ok(())
            } else {
                return Err(OrderError::OrderNotExist);
            }
        }

        fn remove_order(&mut self, maybe_owner: &Account, order_id: Self::OrderId, quantity: Unit) -> Result<(), Self::Error> {
            if let Some(order) = self.open_orders.get_mut(&order_id) {
                ensure!(order.owner == *maybe_owner, OrderError::NotOwner);
                order.quantity = order.quantity.checked_sub(&quantity).ok_or(OrderError::Underflow)?;
                Ok(())
            } else {
                return Err(OrderError::OrderNotExist);
            }
        }
    }
}
