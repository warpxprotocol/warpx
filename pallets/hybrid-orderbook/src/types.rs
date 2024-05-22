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
use sp_core::U256;
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
pub(super) type BalancePath<T> = Vec<(<T as Config>::AssetKind, <T as Config>::Balance)>;

/// Credit of [Config::Assets].
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Assets>;

/// Stores the lp_token asset id a particular pool has been assigned.
#[derive(Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<PoolAssetId> {
	/// Liquidity pool asset
	pub lp_token: PoolAssetId,
}

/// Detail of the pool 
#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    /// Liquidity pool asset
	pub lp_token: T::PoolAssetId,
    /// The orderbook of the bid.
    bids: T::OrderBook,
    /// The orderbook of the ask.
    asks: T::OrderBook,
    next_bid_order_id: OrderId,
    /// The next order id of the ask.
    next_ask_order_id: OrderId,
    /// The fee rate of the taker.
    taker_fee_rate: Permill,
    /// The size of each tick.
    tick_size: T::OrderBookIndex, 
    /// The minimum amount of the order.
    lot_size: T::OrderBookIndex,
}

impl<T: Config> Pool<T> {
    pub fn new(
        lp_token: T::PoolAssetId,
        taker_fee_rate: Permill, 
        tick_size: T::OrderBookIndex,
        lot_size: T::OrderBookIndex,
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

/// Pool locator that mandates the inclusion of the specified `FirstAsset` in every asset pair.
///
/// The `PoolId` is represented as a tuple of `AssetKind`s with `FirstAsset` always positioned as
/// the first element.
pub struct WithFirstAsset<FirstAsset, AccountId, AssetKind, AccountIdConverter>(
	PhantomData<(FirstAsset, AccountId, AssetKind, AccountIdConverter)>,
);
impl<FirstAsset, AccountId, AssetKind, AccountIdConverter>
	PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
	for WithFirstAsset<FirstAsset, AccountId, AssetKind, AccountIdConverter>
where
	AssetKind: Eq + Clone + Encode,
	AccountId: Decode,
	FirstAsset: Get<AssetKind>,
	AccountIdConverter: for<'a> TryConvert<&'a (AssetKind, AssetKind), AccountId>,
{
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
		if asset1 == asset2 {
			return Err(());
		}
		let first = FirstAsset::get();
		if first == *asset1 {
			Ok((first, asset2.clone()))
		} else if first == *asset2 {
			Ok((first, asset1.clone()))
		} else {
			Err(())
		}
	}
	fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
		AccountIdConverter::try_convert(id).map_err(|_| ())
	}
}

/// Pool locator where the `PoolId` is a tuple of `AssetKind`s arranged in ascending order.
pub struct Ascending<AccountId, AssetKind, AccountIdConverter>(
	PhantomData<(AccountId, AssetKind, AccountIdConverter)>,
);
impl<AccountId, AssetKind, AccountIdConverter>
	PoolLocator<AccountId, AssetKind, (AssetKind, AssetKind)>
	for Ascending<AccountId, AssetKind, AccountIdConverter>
where
	AssetKind: Ord + Clone + Encode,
	AccountId: Decode,
	AccountIdConverter: for<'a> TryConvert<&'a (AssetKind, AssetKind), AccountId>,
{
	fn pool_id(asset1: &AssetKind, asset2: &AssetKind) -> Result<(AssetKind, AssetKind), ()> {
		if asset1 > asset2 {
			Ok((asset2.clone(), asset1.clone()))
		} else if asset1 < asset2 {
			Ok((asset1.clone(), asset2.clone()))
		} else {
			Err(())
		}
	}
	fn address(id: &(AssetKind, AssetKind)) -> Result<AccountId, ()> {
		AccountIdConverter::try_convert(id).map_err(|_| ())
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

	pub trait OrderBook {

        type Index: OrderBookIndex;
        type Order;
        /// Identifier for each order
        type OrderId;

        /// Create new instance
        fn new() -> Self;

        fn new_order(&mut self, order: Self::Order) -> Result<(), DispatchError>;

        fn remove_order(&mut self, order_id: Self::OrderId) -> Result<(), DispatchError>;
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
}
