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

//! # Substrate Asset Conversion pallet
//!
//! Substrate Asset Conversion pallet based on the [Uniswap V2](https://github.com/Uniswap/v2-core) logic.
//!
//! ## Overview
//!
//! This pallet allows you to:
//!
//!  - [create a liquidity pool](`Pallet::create_pool()`) for 2 assets
//!  - [provide the liquidity](`Pallet::add_liquidity()`) and receive back an LP token
//!  - [exchange the LP token back to assets](`Pallet::remove_liquidity()`)
//!  - [swap a specific amount of assets for another](`Pallet::swap_exact_tokens_for_tokens()`) if
//!    there is a pool created, or
//!  - [swap some assets for a specific amount of
//!    another](`Pallet::swap_tokens_for_exact_tokens()`).
//!  - [query for an exchange price](`AssetConversionApi::quote_price_exact_tokens_for_tokens`) via
//!    a runtime call endpoint
//!  - [query the size of a liquidity pool](`AssetConversionApi::get_reserves`) via a runtime api
//!    endpoint.
//!
//! The `quote_price_exact_tokens_for_tokens` and `quote_price_tokens_for_exact_tokens` functions
//! both take a path parameter of the route to take. If you want to swap from native asset to
//! non-native asset 1, you would pass in a path of `[DOT, 1]` or `[1, DOT]`. If you want to swap
//! from non-native asset 1 to non-native asset 2, you would pass in a path of `[1, DOT, 2]`.
//!
//! (For an example of configuring this pallet to use `Location` as an asset id, see the
//! cumulus repo).
//!
//! Here is an example `state_call` that asks for a quote of a pool of native versus asset 1:
//!
//! ```text
//! curl -sS -H "Content-Type: application/json" -d \
//! '{"id":1, "jsonrpc":"2.0", "method": "state_call", "params": ["AssetConversionApi_quote_price_tokens_for_exact_tokens", "0x0101000000000000000000000011000000000000000000"]}' \
//! http://localhost:9933/
//! ```
//! (This can be run against the kitchen sync node in the `node` folder of this repo.)
// #![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
mod swap;
#[cfg(test)]
mod tests;
mod types;
mod critbit;
pub mod weights;
#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::{BenchmarkHelper, NativeOrWithIdFactory};
pub use pallet::*;
pub use swap::*;
pub use types::*;
pub use critbit::*;
pub use weights::WeightInfo;

use codec::{Codec, Encode, Decode};
use scale_info::TypeInfo;
use frame_support::{
	storage::{with_storage_layer, with_transaction},
	traits::{
		fungibles::{Balanced, Create, Credit, Inspect, Mutate},
		tokens::{
			AssetId, Balance,
			Fortitude::Polite,
			Precision::Exact,
			Preservation::{Expendable, Preserve},
		},
		AccountTouch, Incrementable, OnUnbalanced,
	},
	PalletId, ensure
};
use sp_core::Get;
use sp_runtime::{
	traits::{
		CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Ensure, IntegerSquareRoot, MaybeDisplay,
		One, TrailingZeroInput, Zero, 
	},
	DispatchError, Saturating, TokenError, TransactionOutcome, Permill
};
use sp_std::{boxed::Box, collections::{btree_set::BTreeSet, btree_map::BTreeMap}, vec::Vec};

#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;
	use sp_arithmetic::{traits::Unsigned, Permill};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type which is used as `key` in `T::OrderBook`, `amount of Reserve`, `quantity of order`, etc..
		type Unit: Balance + OrderBookIndex;

		/// A type used for calculations concerning the `Unit` type to avoid possible overflows.
		type HigherPrecisionUnit: IntegerSquareRoot
			+ One
			+ Ensure
			+ Unsigned
			+ From<u32>
			+ From<Self::Unit>
			+ TryInto<Self::Unit>;

		/// Type of asset class, sourced from [`Config::Assets`], utilized to offer liquidity to a
		/// pool.
		type AssetKind: Parameter + MaxEncodedLen;

		/// Registry of assets utilized for providing liquidity to pools.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetKind, Balance = Self::Unit>
			+ Mutate<Self::AccountId>
			+ AccountTouch<Self::AssetKind, Self::AccountId, Balance = Self::Unit>
			+ Balanced<Self::AccountId>;

		/// Type of data structure of orderbook
		type OrderBook: OrderBook + Parameter;

		/// Liquidity pool identifier.
		type PoolId: Parameter + MaxEncodedLen + Ord;

		/// Provides means to resolve the [`Config::PoolId`] and it's `AccountId` from a pair
		/// of [`Config::AssetKind`]s.
		///
		/// Examples: [`crate::types::WithFirstAsset`], [`crate::types::Ascending`].
		type PoolLocator: PoolLocator<Self::AccountId, Self::AssetKind, Self::PoolId>;

		/// Asset class for the lp tokens from [`Self::PoolAssets`].
		type PoolAssetId: AssetId + PartialOrd + Incrementable + From<u32>;

		/// Registry for the lp tokens. Ideally only this pallet should have create permissions on
		/// the assets.
		type PoolAssets: Inspect<Self::AccountId, AssetId = Self::PoolAssetId, Balance = Self::Unit>
			+ Create<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ AccountTouch<Self::PoolAssetId, Self::AccountId, Balance = Self::Unit>;

		/// A % the liquidity providers will take of every swap. Represents 10ths of a percent.
		#[pallet::constant]
		type LPFee: Get<u32>;

		/// A one-time fee to setup the pool.
		#[pallet::constant]
		type PoolSetupFee: Get<Self::Unit>;

		/// Asset class from [`Config::Assets`] used to pay the [`Config::PoolSetupFee`].
		#[pallet::constant]
		type PoolSetupFeeAsset: Get<Self::AssetKind>;

		/// Handler for the [`Config::PoolSetupFee`].
		type PoolSetupFeeTarget: OnUnbalanced<CreditOf<Self>>;

		/// A fee to withdraw the liquidity.
		#[pallet::constant]
		type LiquidityWithdrawalFee: Get<Permill>;

		/// The minimum LP token amount that could be minted. Ameliorates rounding errors.
		#[pallet::constant]
		type MintMinLiquidity: Get<Self::Unit>;

		/// The max number of hops in a swap.
		#[pallet::constant]
		type MaxSwapPathLength: Get<u32>;

		/// The pallet's id, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The benchmarks need a way to create asset ids from u32s.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<Self::AssetKind>;
	}

	/// Map from `PoolAssetId` to `PoolInfo`. This establishes whether a pool has been officially
	/// created rather than people sending tokens directly to a pool's public account.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type Pools<T: Config> =
		StorageMap<_, Blake2_128Concat, T::PoolId, Pool<T>, OptionQuery>;

	/// Stores the `PoolAssetId` that is going to be used for the next lp token.
	/// This gets incremented whenever a new lp pool is created.
	#[pallet::storage]
	pub type NextPoolAssetId<T: Config> = StorageValue<_, T::PoolAssetId, OptionQuery>;

	// Pallet's events.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A successful call of the `CreatePool` extrinsic will create this event.
		PoolCreated {
			/// The account that created the pool.
			creator: T::AccountId,
			/// The pool id associated with the pool. Note that the order of the assets may not be
			/// the same as the order specified in the create pool extrinsic.
			pool_id: T::PoolId,
			/// The account ID of the pool.
			pool_account: T::AccountId,
			/// The id of the liquidity tokens that will be minted when assets are added to this
			/// pool.
			lp_token: T::PoolAssetId,
			/// The fee rate of the taker.
			taker_fee_rate: Permill,
			/// The tick size of the orderbook.
			tick_size: T::Unit,
			/// The lot size of the orderbook.
			lot_size: T::Unit,
		},

		/// A successful call of the `AddLiquidity` extrinsic will create this event.
		LiquidityAdded {
			/// The account that the liquidity was taken from.
			who: T::AccountId,
			/// The account that the liquidity tokens were minted to.
			mint_to: T::AccountId,
			/// The pool id of the pool that the liquidity was added to.
			pool_id: T::PoolId,
			/// The amount of the first asset that was added to the pool.
			base_asset_provided: T::Unit,
			/// The amount of the second asset that was added to the pool.
			quote_asset_provided: T::Unit,
			/// The id of the lp token that was minted.
			lp_token: T::PoolAssetId,
			/// The amount of lp tokens that were minted of that id.
			lp_token_minted: T::Unit,
		},

		/// A successful call of the `RemoveLiquidity` extrinsic will create this event.
		LiquidityRemoved {
			/// The account that the liquidity tokens were burned from.
			who: T::AccountId,
			/// The account that the assets were transferred to.
			withdraw_to: T::AccountId,
			/// The pool id that the liquidity was removed from.
			pool_id: T::PoolId,
			/// The amount of the first asset that was removed from the pool.
			base_asset_amount: T::Unit,
			/// The amount of the second asset that was removed from the pool.
			quote_asset_amount: T::Unit,
			/// The id of the lp token that was burned.
			lp_token: T::PoolAssetId,
			/// The amount of lp tokens that were burned of that id.
			lp_token_burned: T::Unit,
			/// Liquidity withdrawal fee (%).
			withdrawal_fee: Permill,
		},
		/// Assets have been converted from one to another. Both `SwapExactTokenForToken`
		/// and `SwapTokenForExactToken` will generate this event.
		SwapExecuted {
			/// Which account was the instigator of the swap.
			who: T::AccountId,
			/// The account that the assets were transferred to.
			send_to: T::AccountId,
			/// The amount of the first asset that was swapped.
			amount_in: T::Unit,
			/// The amount of the second asset that was received.
			amount_out: T::Unit,
			/// The route of asset IDs with amounts that the swap went through.
			/// E.g. (A, amount_in) -> (Dot, amount_out) -> (B, amount_out)
			path: BalancePath<T>,
		},
		/// Assets have been converted from one to another.
		SwapCreditExecuted {
			/// The amount of the first asset that was swapped.
			amount_in: T::Unit,
			/// The amount of the second asset that was received.
			amount_out: T::Unit,
			/// The route of asset IDs with amounts that the swap went through.
			/// E.g. (A, amount_in) -> (Dot, amount_out) -> (B, amount_out)
			path: BalancePath<T>,
		},
		/// Pool has been touched in order to fulfill operational requirements.
		Touched {
			/// The ID of the pool.
			pool_id: T::PoolId,
			/// The account initiating the touch.
			who: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Provided asset pair is not supported for pool.
		InvalidAssetPair,
		/// Pool already exists.
		PoolExists,
		/// Desired amount can't be zero.
		WrongDesiredAmount,
		/// Provided amount should be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		AmountOneLessThanMinimal,
		/// Provided amount should be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		AmountTwoLessThanMinimal,
		/// Reserve needs to always be greater than or equal to the existential deposit/asset's
		/// minimal amount.
		ReserveLeftLessThanMinimal,
		/// Desired amount can't be equal to the pool reserve.
		AmountOutTooHigh,
		/// The pool doesn't exist.
		PoolNotFound,
		/// An overflow happened.
		Overflow,
		/// The minimal amount requirement for the first token in the pair wasn't met.
		AssetOneDepositDidNotMeetMinimum,
		/// The minimal amount requirement for the second token in the pair wasn't met.
		AssetTwoDepositDidNotMeetMinimum,
		/// The minimal amount requirement for the first token in the pair wasn't met.
		AssetOneWithdrawalDidNotMeetMinimum,
		/// The minimal amount requirement for the second token in the pair wasn't met.
		AssetTwoWithdrawalDidNotMeetMinimum,
		/// Optimal calculated amount is less than desired.
		OptimalAmountLessThanDesired,
		/// Insufficient liquidity minted.
		InsufficientLiquidityMinted,
		/// Requested liquidity can't be zero.
		ZeroLiquidity,
		/// Amount can't be zero.
		ZeroAmount,
		/// Calculated amount out is less than provided minimum amount.
		ProvidedMinimumNotSufficientForSwap,
		/// Provided maximum amount is not sufficient for swap.
		ProvidedMaximumNotSufficientForSwap,
		/// The provided path must consists of 2 assets at least.
		InvalidPath,
		/// The provided path must consists of unique assets.
		NonUniquePath,
		/// It was not possible to get or increment the Id of the pool.
		IncorrectPoolAssetId,
		/// The destination account cannot exist with the swapped funds.
		BelowMinimum,
		/// The order price must be a multiple of the tick size.
		InvalidOrderPrice,
		/// The order quantity must be a multiple of the lot size.
		InvalidOrderQuantity,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(
				T::MaxSwapPathLength::get() > 1,
				"the `MaxSwapPathLength` should be greater than 1",
			);
		}
	}

	/// Pallet's callable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates an empty liquidity pool and an associated new `lp_token` asset
		/// (the id of which is returned in the `Event::PoolCreated` event).
		///
		/// Once a pool is created, someone may [`Pallet::add_liquidity`] to it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_pool())]
		pub fn create_pool(
			origin: OriginFor<T>,
			base_asset: Box<T::AssetKind>,
			quote_asset: Box<T::AssetKind>,
			taker_fee_rate: Permill,
			tick_size: T::Unit,
			lot_size: T::Unit,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(base_asset != quote_asset, Error::<T>::InvalidAssetPair);

			// prepare pool_id
			let pool_id = T::PoolLocator::pool_id(&base_asset, &quote_asset)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;
			ensure!(!Pools::<T>::contains_key(&pool_id), Error::<T>::PoolExists);

			let pool_account =
				T::PoolLocator::address(&pool_id).map_err(|_| Error::<T>::InvalidAssetPair)?;

			// pay the setup fee
			let fee =
				Self::withdraw(T::PoolSetupFeeAsset::get(), &sender, T::PoolSetupFee::get(), true)?;
			T::PoolSetupFeeTarget::on_unbalanced(fee);

			if T::Assets::should_touch(*base_asset.clone(), &pool_account) {
				T::Assets::touch(*base_asset, &pool_account, &sender)?
			};

			if T::Assets::should_touch(*quote_asset.clone(), &pool_account) {
				T::Assets::touch(*quote_asset, &pool_account, &sender)?
			};

			let lp_token = NextPoolAssetId::<T>::get()
				.or(T::PoolAssetId::initial_value())
				.ok_or(Error::<T>::IncorrectPoolAssetId)?;
			let next_lp_token_id = lp_token.increment().ok_or(Error::<T>::IncorrectPoolAssetId)?;
			NextPoolAssetId::<T>::set(Some(next_lp_token_id));

			T::PoolAssets::create(lp_token.clone(), pool_account.clone(), false, 1u32.into())?;
			if T::PoolAssets::should_touch(lp_token.clone(), &pool_account) {
				T::PoolAssets::touch(lp_token.clone(), &pool_account, &sender)?
			};

			let pool = Pool::<T>::new(lp_token.clone(), taker_fee_rate, tick_size, lot_size);
			Pools::<T>::insert(pool_id.clone(), pool);

			Self::deposit_event(Event::PoolCreated {
				creator: sender,
				pool_id,
				pool_account,
				lp_token,
				taker_fee_rate,
				tick_size,
				lot_size
			});

			Ok(())
		}

		/// Provide liquidity into the pool of `asset1` and `asset2`.
		/// NOTE: an optimal amount of asset1 and asset2 will be calculated and
		/// might be different than the provided `amount1_desired`/`amount2_desired`
		/// thus you should provide the min amount you're happy to provide.
		/// Params `amount1_min`/`amount2_min` represent that.
		/// `mint_to` will be sent the liquidity tokens that represent this share of the pool.
		///
		/// NOTE: when encountering an incorrect exchange rate and non-withdrawable pool liquidity,
		/// batch an atomic call with [`Pallet::add_liquidity`] and
		/// [`Pallet::swap_exact_tokens_for_tokens`] or [`Pallet::swap_tokens_for_exact_tokens`]
		/// calls to render the liquidity withdrawable and rectify the exchange rate.
		///
		/// Once liquidity is added, someone may successfully call
		/// [`Pallet::swap_exact_tokens_for_tokens`] successfully.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::add_liquidity())]
		pub fn add_liquidity(
			origin: OriginFor<T>,
			base_asset: Box<T::AssetKind>,
			quote_asset: Box<T::AssetKind>,
			base_asset_desired: T::Unit,
			quote_asset_desired: T::Unit,
			base_asset_min: T::Unit,
			quote_asset_min: T::Unit,
			mint_to: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let pool_id = T::PoolLocator::pool_id(&base_asset, &quote_asset)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;

			ensure!(
				base_asset_desired > Zero::zero() && quote_asset_desired > Zero::zero(),
				Error::<T>::WrongDesiredAmount
			);

			let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound)?;
			let pool_account =
				T::PoolLocator::address(&pool_id).map_err(|_| Error::<T>::InvalidAssetPair)?;

			let base_asset_reserve = Self::get_balance(&pool_account, *base_asset.clone());
			let quote_asset_reserve = Self::get_balance(&pool_account, *quote_asset.clone());

			let base_asset_amount: T::Unit;
			let quote_asset_amount: T::Unit;
			if base_asset_reserve.is_zero() || quote_asset_reserve.is_zero() {
				base_asset_amount = base_asset_desired;
				quote_asset_amount = quote_asset_desired;
			} else {
				let quote_asset_optimal = Self::quote(&base_asset_desired, &base_asset_reserve, &quote_asset_reserve)?;

				if quote_asset_optimal <= quote_asset_desired {
					ensure!(
						quote_asset_optimal >= quote_asset_min,
						Error::<T>::AssetTwoDepositDidNotMeetMinimum
					);
					base_asset_amount = base_asset_desired;
					quote_asset_amount = quote_asset_optimal;
				} else {
					let base_asset_optimal = Self::quote(&quote_asset_desired, &quote_asset_reserve, &base_asset_reserve)?;
					ensure!(
						base_asset_optimal <= base_asset_desired,
						Error::<T>::OptimalAmountLessThanDesired
					);
					ensure!(
						base_asset_optimal >= base_asset_min,
						Error::<T>::AssetOneDepositDidNotMeetMinimum
					);
					base_asset_amount = base_asset_optimal;
					quote_asset_amount = quote_asset_desired;
				}
			}

			ensure!(
				base_asset_amount.saturating_add(base_asset_reserve) >= T::Assets::minimum_balance(*base_asset.clone()),
				Error::<T>::AmountOneLessThanMinimal
			);
			ensure!(
				quote_asset_amount.saturating_add(quote_asset_reserve) >= T::Assets::minimum_balance(*quote_asset.clone()),
				Error::<T>::AmountTwoLessThanMinimal
			);

			T::Assets::transfer(*base_asset, &sender, &pool_account, base_asset_amount, Preserve)?;
			T::Assets::transfer(*quote_asset, &sender, &pool_account, quote_asset_amount, Preserve)?;

			let total_supply = T::PoolAssets::total_issuance(pool.lp_token.clone());

			let lp_token_amount: T::Unit;
			if total_supply.is_zero() {
				lp_token_amount = Self::calc_lp_amount_for_zero_supply(&base_asset_amount, &quote_asset_amount)?;
				T::PoolAssets::mint_into(
					pool.lp_token.clone(),
					&pool_account,
					T::MintMinLiquidity::get(),
				)?;
			} else {
				let side1 = Self::mul_div(&base_asset_amount, &total_supply, &base_asset_reserve)?;
				let side2 = Self::mul_div(&quote_asset_amount, &total_supply, &quote_asset_reserve)?;
				lp_token_amount = side1.min(side2);
			}

			ensure!(
				lp_token_amount > T::MintMinLiquidity::get(),
				Error::<T>::InsufficientLiquidityMinted
			);

			T::PoolAssets::mint_into(pool.lp_token.clone(), &mint_to, lp_token_amount)?;

			Self::deposit_event(Event::LiquidityAdded {
				who: sender,
				mint_to,
				pool_id,
				base_asset_provided: base_asset_amount,
				quote_asset_provided: quote_asset_amount,
				lp_token: pool.lp_token,
				lp_token_minted: lp_token_amount,
			});

			Ok(())
		}

		/// Allows you to remove liquidity by providing the `lp_token_burn` tokens that will be
		/// burned in the process. With the usage of `amount1_min_receive`/`amount2_min_receive`
		/// it's possible to control the min amount of returned tokens you're happy with.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::remove_liquidity())]
		pub fn remove_liquidity(
			origin: OriginFor<T>,
			base_asset: Box<T::AssetKind>,
			quote_asset: Box<T::AssetKind>,
			lp_token_burn: T::Unit,
			base_asset_min_receive: T::Unit,
			quote_asset_min_receive: T::Unit,
			withdraw_to: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let pool_id = T::PoolLocator::pool_id(&base_asset, &quote_asset)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;

			ensure!(lp_token_burn > Zero::zero(), Error::<T>::ZeroLiquidity);

			let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound)?;

			let pool_account =
				T::PoolLocator::address(&pool_id).map_err(|_| Error::<T>::InvalidAssetPair)?;
			let base_asset_reserve = Self::get_balance(&pool_account, *base_asset.clone());
			let quote_asset_reserve = Self::get_balance(&pool_account, *quote_asset.clone());

			let total_supply = T::PoolAssets::total_issuance(pool.lp_token.clone());
			let withdrawal_fee_amount = T::LiquidityWithdrawalFee::get() * lp_token_burn;
			let lp_redeem_amount = lp_token_burn.saturating_sub(withdrawal_fee_amount);

			let base_asset_amount = Self::mul_div(&lp_redeem_amount, &base_asset_reserve, &total_supply)?;
			let quote_asset_amount = Self::mul_div(&lp_redeem_amount, &quote_asset_reserve, &total_supply)?;

			ensure!(
				!base_asset_amount.is_zero() && base_asset_amount >= base_asset_min_receive,
				Error::<T>::AssetOneWithdrawalDidNotMeetMinimum
			);
			ensure!(
				!quote_asset_amount.is_zero() && quote_asset_amount >= quote_asset_min_receive,
				Error::<T>::AssetTwoWithdrawalDidNotMeetMinimum
			);
			let base_asset_reserve_left = base_asset_reserve.saturating_sub(base_asset_amount);
			let quote_asset_reserve_left = quote_asset_reserve.saturating_sub(quote_asset_amount);
			ensure!(
				base_asset_reserve_left >= T::Assets::minimum_balance(*base_asset.clone()),
				Error::<T>::ReserveLeftLessThanMinimal
			);
			ensure!(
				quote_asset_reserve_left >= T::Assets::minimum_balance(*quote_asset.clone()),
				Error::<T>::ReserveLeftLessThanMinimal
			);

			// burn the provided lp token amount that includes the fee
			T::PoolAssets::burn_from(pool.lp_token.clone(), &sender, lp_token_burn, Exact, Polite)?;

			T::Assets::transfer(*base_asset, &pool_account, &withdraw_to, base_asset_amount, Expendable)?;
			T::Assets::transfer(*quote_asset, &pool_account, &withdraw_to, quote_asset_amount, Expendable)?;

			Self::deposit_event(Event::LiquidityRemoved {
				who: sender,
				withdraw_to,
				pool_id,
				base_asset_amount,
				quote_asset_amount,
				lp_token: pool.lp_token,
				lp_token_burned: lp_token_burn,
				withdrawal_fee: T::LiquidityWithdrawalFee::get(),
			});

			Ok(())
		}

		/// Swap the exact amount of `asset1` into `asset2`.
		/// `amount_out_min` param allows you to specify the min amount of the `asset2`
		/// you're happy to receive.
		///
		/// [`AssetConversionApi::quote_price_exact_tokens_for_tokens`] runtime call can be called
		/// for a quote.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::swap_exact_tokens_for_tokens(path.len() as u32))]
		pub fn swap_exact_tokens_for_tokens(
			origin: OriginFor<T>,
			path: Vec<Box<T::AssetKind>>,
			amount_in: T::Unit,
			amount_out_min: T::Unit,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Self::do_swap_exact_tokens_for_tokens(
				sender,
				path.into_iter().map(|a| *a).collect(),
				amount_in,
				Some(amount_out_min),
				send_to,
				keep_alive,
			)?;
			Ok(())
		}

		/// Swap any amount of `asset1` to get the exact amount of `asset2`.
		/// `amount_in_max` param allows to specify the max amount of the `asset1`
		/// you're happy to provide.
		///
		/// [`AssetConversionApi::quote_price_tokens_for_exact_tokens`] runtime call can be called
		/// for a quote.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::swap_tokens_for_exact_tokens(path.len() as u32))]
		pub fn swap_tokens_for_exact_tokens(
			origin: OriginFor<T>,
			path: Vec<Box<T::AssetKind>>,
			amount_out: T::Unit,
			amount_in_max: T::Unit,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			Self::do_swap_tokens_for_exact_tokens(
				sender,
				path.into_iter().map(|a| *a).collect(),
				amount_out,
				Some(amount_in_max),
				send_to,
				keep_alive,
			)?;
			Ok(())
		}

		/// Touch an existing pool to fulfill prerequisites before providing liquidity, such as
		/// ensuring that the pool's accounts are in place. It is typically useful when a pool
		/// creator removes the pool's accounts and does not provide a liquidity. This action may
		/// involve holding assets from the caller as a deposit for creating the pool's accounts.
		///
		/// The origin must be Signed.
		///
		/// - `asset1`: The asset ID of an existing pool with a pair (asset1, asset2).
		/// - `asset2`: The asset ID of an existing pool with a pair (asset1, asset2).
		///
		/// Emits `Touched` event when successful.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::touch(3))]
		pub fn touch(
			origin: OriginFor<T>,
			base_asset: Box<T::AssetKind>,
			quote_asset: Box<T::AssetKind>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let pool_id = T::PoolLocator::pool_id(&base_asset, &quote_asset)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;
			let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound)?;
			let pool_account =
				T::PoolLocator::address(&pool_id).map_err(|_| Error::<T>::InvalidAssetPair)?;

			let mut refunds_number: u32 = 0;
			if T::Assets::should_touch(*base_asset.clone(), &pool_account) {
				T::Assets::touch(*base_asset, &pool_account, &who)?;
				refunds_number += 1;
			}
			if T::Assets::should_touch(*quote_asset.clone(), &pool_account) {
				T::Assets::touch(*quote_asset, &pool_account, &who)?;
				refunds_number += 1;
			}
			if T::PoolAssets::should_touch(pool.lp_token.clone(), &pool_account) {
				T::PoolAssets::touch(pool.lp_token, &pool_account, &who)?;
				refunds_number += 1;
			}
			Self::deposit_event(Event::Touched { pool_id, who });
			Ok(Some(T::WeightInfo::touch(refunds_number)).into())
		}

		// TODO: Benchmark
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::touch(3))]
		pub fn place_market_order(
			origin: OriginFor<T>, 			
			base_asset: Box<T::AssetKind>,
			quote_asset: Box<T::AssetKind>,
			quantity: T::Unit,
		) -> DispatchResult {
			let taker = ensure_signed(origin)?;
			ensure!(quantity > Zero::zero(), Error::<T>::WrongDesiredAmount);
			let pool = Self::get_pool(base_asset, quote_asset)?;
			Self::do_place_market_order(taker, pool)?;
			Ok(())
		}
		
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::touch(3))]
		pub fn place_limit_order(
			origin: OriginFor<T>, 
			base_asset: Box<T::AssetKind>, 
			quote_asset: Box<T::AssetKind>, 
			is_bid: bool,
			price: T::Unit,
			quantity: T::Unit,
		) -> DispatchResult {
			let maker = ensure_signed(origin)?;
			Self::do_place_limit_order(maker, price, quantity, is_bid, base_asset, quote_asset)?;
			Ok(())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::touch(3))]
		pub fn cancel_order(origin: OriginFor<T>, 
			base_asset: Box<T::AssetKind>, 
			quote_asset: Box<T::AssetKind>
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let pool = Self::get_pool(base_asset, quote_asset)?;
			Self::do_cancel_order(owner, pool)?;
			Ok(())
		}

		// impl me!
		// #[pallet::call_index(9)]
		// #[pallet::weight(T::WeightInfo::touch(3))]
		// pub fn stop_limit_order(origin: OriginFor<T>) -> DispatchResult {
		// 	let owner = ensure_signed(origin)?;
		// 	let pool = Self::get_pool(asset1, asset2)?;
		// 	Self::do_stop_limit_order(owner, pool)?;
		// 	Ok(())
		// }
	}

	impl<T: Config> Pallet<T> {

		fn get_pool(base_asset: Box<T::AssetKind>, quote_asset: Box<T::AssetKind>) -> Result<Pool<T>, DispatchError> {
			let pool_id = T::PoolLocator::pool_id(&base_asset, &quote_asset)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;
			Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound.into())
		}

		pub(crate) fn do_place_market_order(_taker: T::AccountId, _pool: Pool<T>) -> DispatchResult {
			Ok(())
		}

		pub(crate) fn do_place_limit_order(
			maker: T::AccountId, 
			order_price: T::Unit, 
			order_quantity: T::Unit, 
			is_bid: bool, 
			base_asset: Box<T::AssetKind>, 
			quote_asset: Box<T::AssetKind>
		) -> DispatchResult {
			// Validity check
			let pool = Self::get_pool(base_asset.clone(), quote_asset.clone())?;
			ensure!(order_quantity > Zero::zero(), Error::<T>::WrongDesiredAmount);
			ensure!(order_price > Zero::zero(), Error::<T>::InvalidOrderPrice);
			ensure!(order_price % pool.tick_size == Zero::zero(), Error::<T>::InvalidOrderPrice);
			ensure!(order_quantity >= pool.lot_size && order_quantity % pool.lot_size == Zero::zero(), Error::<T>::InvalidOrderQuantity);
			if is_bid {
				Self::match_bid(maker, *base_asset, *quote_asset, order_price, order_quantity)?;
			} else {
				Self::match_ask(maker, *base_asset, *quote_asset, order_price, order_quantity)?;
			}
			Ok(())
		}

		pub(crate) fn do_cancel_order(_owner: T::AccountId, _pool: Pool<T>) -> DispatchResult {
			Ok(())
		}

		pub(crate) fn do_stop_limit_order(_owner: T::AccountId, _pool: Pool<T>) -> DispatchResult {
			Ok(())
		}

		pub(crate) fn match_bid(maker: T::AccountId, base_asset: T::AssetKind, quote_asset: T::AssetKind, order_price: T::Unit, order_quantity: T::Unit) -> Result<(), DispatchError> {
			let last_price = Self::last_price(base_asset, quote_asset)?;
			if order_price >= last_price {
				// Fill the order
				let amount_in = Self::get_amount_in(order_quantity, )
			} else {
				// Push the order to the orderbook
			}
			Ok(())
		}
		
		pub(crate) fn match_ask(maker: T::AccountId, base_asset: T::AssetKind, quote_asset: T::AssetKind, order_price: T::Unit, order_quantity: T::Unit) -> Result<(), DispatchError> {
			let last_price = Self::last_price(base_asset, quote_asset)?;
			if order_price <= last_price {
				// Fill the order
			} else {
				// Push the order to the orderbook
			}
			Ok(())
		} 

		/// Current price of the pool based on base and quote reserves
		pub(crate) fn last_price(base_asset: T::AssetKind, quote_asset: T::AssetKind) -> Result<T::Unit, DispatchError> {
			let (base_reserve, quote_reserve) = Self::get_reserves(base_asset, quote_asset)?;
			Ok(quote_reserve / base_reserve)
		}

		/// Swap exactly `amount_in` of asset `path[0]` for asset `path[1]`.
		/// If an `amount_out_min` is specified, it will return an error if it is unable to acquire
		/// the amount desired.
		///
		/// Withdraws the `path[0]` asset from `sender`, deposits the `path[1]` asset to `send_to`,
		/// respecting `keep_alive`.
		///
		/// If successful, returns the amount of `path[1]` acquired for the `amount_in`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_exact_tokens_for_tokens(
			sender: T::AccountId,
			path: Vec<T::AssetKind>,
			amount_in: T::Unit,
			amount_out_min: Option<T::Unit>,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> Result<T::Unit, DispatchError> {
			ensure!(amount_in > Zero::zero(), Error::<T>::ZeroAmount);
			if let Some(amount_out_min) = amount_out_min {
				ensure!(amount_out_min > Zero::zero(), Error::<T>::ZeroAmount);
			}

			Self::validate_swap_path(&path)?;
			let path = Self::balance_path_from_amount_in(amount_in, path)?;

			let amount_out = path.last().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
			if let Some(amount_out_min) = amount_out_min {
				ensure!(
					amount_out >= amount_out_min,
					Error::<T>::ProvidedMinimumNotSufficientForSwap
				);
			}

			Self::swap(&sender, &path, &send_to, keep_alive)?;

			Self::deposit_event(Event::SwapExecuted {
				who: sender,
				send_to,
				amount_in,
				amount_out,
				path,
			});
			Ok(amount_out)
		}

		/// Take the `path[0]` asset and swap some amount for `amount_out` of the `path[1]`. If an
		/// `amount_in_max` is specified, it will return an error if acquiring `amount_out` would be
		/// too costly.
		///
		/// Withdraws `path[0]` asset from `sender`, deposits the `path[1]` asset to `send_to`,
		/// respecting `keep_alive`.
		///
		/// If successful returns the amount of the `path[0]` taken to provide `path[1]`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_tokens_for_exact_tokens(
			sender: T::AccountId,
			path: Vec<T::AssetKind>,
			amount_out: T::Unit,
			amount_in_max: Option<T::Unit>,
			send_to: T::AccountId,
			keep_alive: bool,
		) -> Result<T::Unit, DispatchError> {
			ensure!(amount_out > Zero::zero(), Error::<T>::ZeroAmount);
			if let Some(amount_in_max) = amount_in_max {
				ensure!(amount_in_max > Zero::zero(), Error::<T>::ZeroAmount);
			}

			Self::validate_swap_path(&path)?;
			let path = Self::balance_path_from_amount_out(amount_out, path)?;

			let amount_in = path.first().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
			if let Some(amount_in_max) = amount_in_max {
				ensure!(
					amount_in <= amount_in_max,
					Error::<T>::ProvidedMaximumNotSufficientForSwap
				);
			}

			Self::swap(&sender, &path, &send_to, keep_alive)?;

			Self::deposit_event(Event::SwapExecuted {
				who: sender,
				send_to,
				amount_in,
				amount_out,
				path,
			});

			Ok(amount_in)
		}

		/// Swap exactly `credit_in` of asset `path[0]` for asset `path[last]`.  If `amount_out_min`
		/// is provided and the swap can't achieve at least this amount, an error is returned.
		///
		/// On a successful swap, the function returns the `credit_out` of `path[last]` obtained
		/// from the `credit_in`. On failure, it returns an `Err` containing the original
		/// `credit_in` and the associated error code.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_exact_credit_tokens_for_tokens(
			path: Vec<T::AssetKind>,
			credit_in: CreditOf<T>,
			amount_out_min: Option<T::Unit>,
		) -> Result<CreditOf<T>, (CreditOf<T>, DispatchError)> {
			let amount_in = credit_in.peek();
			let inspect_path = |credit_asset| {
				ensure!(
					path.first().map_or(false, |a| *a == credit_asset),
					Error::<T>::InvalidPath
				);
				ensure!(!amount_in.is_zero(), Error::<T>::ZeroAmount);
				ensure!(amount_out_min.map_or(true, |a| !a.is_zero()), Error::<T>::ZeroAmount);

				Self::validate_swap_path(&path)?;
				let path = Self::balance_path_from_amount_in(amount_in, path)?;

				let amount_out = path.last().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
				ensure!(
					amount_out_min.map_or(true, |a| amount_out >= a),
					Error::<T>::ProvidedMinimumNotSufficientForSwap
				);
				Ok((path, amount_out))
			};
			let (path, amount_out) = match inspect_path(credit_in.asset()) {
				Ok((p, a)) => (p, a),
				Err(e) => return Err((credit_in, e)),
			};

			let credit_out = Self::credit_swap(credit_in, &path)?;

			Self::deposit_event(Event::SwapCreditExecuted { amount_in, amount_out, path });

			Ok(credit_out)
		}

		/// Swaps a portion of `credit_in` of `path[0]` asset to obtain the desired `amount_out` of
		/// the `path[last]` asset. The provided `credit_in` must be adequate to achieve the target
		/// `amount_out`, or an error will occur.
		///
		/// On success, the function returns a (`credit_out`, `credit_change`) tuple, where
		/// `credit_out` represents the acquired amount of the `path[last]` asset, and
		/// `credit_change` is the remaining portion from the `credit_in`. On failure, an `Err` with
		/// the initial `credit_in` and error code is returned.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		pub(crate) fn do_swap_credit_tokens_for_exact_tokens(
			path: Vec<T::AssetKind>,
			credit_in: CreditOf<T>,
			amount_out: T::Unit,
		) -> Result<(CreditOf<T>, CreditOf<T>), (CreditOf<T>, DispatchError)> {
			let amount_in_max = credit_in.peek();
			let inspect_path = |credit_asset| {
				ensure!(
					path.first().map_or(false, |a| a == &credit_asset),
					Error::<T>::InvalidPath
				);
				ensure!(amount_in_max > Zero::zero(), Error::<T>::ZeroAmount);
				ensure!(amount_out > Zero::zero(), Error::<T>::ZeroAmount);

				Self::validate_swap_path(&path)?;
				let path = Self::balance_path_from_amount_out(amount_out, path)?;

				let amount_in = path.first().map(|(_, a)| *a).ok_or(Error::<T>::InvalidPath)?;
				ensure!(
					amount_in <= amount_in_max,
					Error::<T>::ProvidedMaximumNotSufficientForSwap
				);

				Ok((path, amount_in))
			};
			let (path, amount_in) = match inspect_path(credit_in.asset()) {
				Ok((p, a)) => (p, a),
				Err(e) => return Err((credit_in, e)),
			};

			let (credit_in, credit_change) = credit_in.split(amount_in);
			let credit_out = Self::credit_swap(credit_in, &path)?;

			Self::deposit_event(Event::SwapCreditExecuted { amount_in, amount_out, path });

			Ok((credit_out, credit_change))
		}

		/// Swap assets along the `path`, withdrawing from `sender` and depositing in `send_to`.
		///
		/// Note: It's assumed that the provided `path` is valid.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		fn swap(
			sender: &T::AccountId,
			path: &BalancePath<T>,
			send_to: &T::AccountId,
			keep_alive: bool,
		) -> Result<(), DispatchError> {
			let (asset_in, amount_in) = path.first().ok_or(Error::<T>::InvalidPath)?;
			let credit_in = Self::withdraw(asset_in.clone(), sender, *amount_in, keep_alive)?;

			let credit_out = Self::credit_swap(credit_in, path).map_err(|(_, e)| e)?;
			T::Assets::resolve(send_to, credit_out).map_err(|_| Error::<T>::BelowMinimum)?;

			Ok(())
		}

		/// Swap assets along the specified `path`, consuming `credit_in` and producing
		/// `credit_out`.
		///
		/// If an error occurs, `credit_in` is returned back.
		///
		/// Note: It's assumed that the provided `path` is valid and `credit_in` corresponds to the
		/// first asset in the `path`.
		///
		/// WARNING: This may return an error after a partial storage mutation. It should be used
		/// only inside a transactional storage context and an Err result must imply a storage
		/// rollback.
		fn credit_swap(
			credit_in: CreditOf<T>,
			path: &BalancePath<T>,
		) -> Result<CreditOf<T>, (CreditOf<T>, DispatchError)> {
			let resolve_path = || -> Result<CreditOf<T>, DispatchError> {
				for pos in 0..=path.len() {
					if let Some([(asset1, _), (asset2, amount_out)]) = path.get(pos..=pos + 1) {
						let pool_from = T::PoolLocator::pool_address(asset1, asset2)
							.map_err(|_| Error::<T>::InvalidAssetPair)?;

						if let Some((asset3, _)) = path.get(pos + 2) {
							let pool_to = T::PoolLocator::pool_address(asset2, asset3)
								.map_err(|_| Error::<T>::InvalidAssetPair)?;

							T::Assets::transfer(
								asset2.clone(),
								&pool_from,
								&pool_to,
								*amount_out,
								Preserve,
							)?;
						} else {
							let credit_out =
								Self::withdraw(asset2.clone(), &pool_from, *amount_out, true)?;
							return Ok(credit_out)
						}
					}
				}
				Err(Error::<T>::InvalidPath.into())
			};

			let credit_out = match resolve_path() {
				Ok(c) => c,
				Err(e) => return Err((credit_in, e)),
			};

			let pool_to = if let Some([(asset1, _), (asset2, _)]) = path.get(0..2) {
				match T::PoolLocator::pool_address(asset1, asset2) {
					Ok(address) => address,
					Err(_) => return Err((credit_in, Error::<T>::InvalidAssetPair.into())),
				}
			} else {
				return Err((credit_in, Error::<T>::InvalidPath.into()))
			};

			T::Assets::resolve(&pool_to, credit_in)
				.map_err(|c| (c, Error::<T>::BelowMinimum.into()))?;

			Ok(credit_out)
		}

		/// Removes `value` balance of `asset` from `who` account if possible.
		fn withdraw(
			asset: T::AssetKind,
			who: &T::AccountId,
			value: T::Unit,
			keep_alive: bool,
		) -> Result<CreditOf<T>, DispatchError> {
			let preservation = match keep_alive {
				true => Preserve,
				false => Expendable,
			};
			if preservation == Preserve {
				// TODO drop the ensure! when this issue addressed
				// https://github.com/paritytech/polkadot-sdk/issues/1698
				let free = T::Assets::reducible_balance(asset.clone(), who, preservation, Polite);
				ensure!(free >= value, TokenError::NotExpendable);
			}
			T::Assets::withdraw(asset, who, value, Exact, preservation, Polite)
		}

		/// Get the `owner`'s balance of `asset`, which could be the chain's native asset or another
		/// fungible. Returns a value in the form of an `Balance`.
		fn get_balance(owner: &T::AccountId, asset: T::AssetKind) -> T::Unit {
			T::Assets::reducible_balance(asset, owner, Expendable, Polite)
		}

		/// Returns the balance of each asset in the pool.
		/// The tuple result is in the order requested (not necessarily the same as pool order).
		pub fn get_reserves(
			asset1: T::AssetKind,
			asset2: T::AssetKind,
		) -> Result<(T::Unit, T::Unit), Error<T>> {
			let pool_account = T::PoolLocator::pool_address(&asset1, &asset2)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;

			let balance1 = Self::get_balance(&pool_account, asset1);
			let balance2 = Self::get_balance(&pool_account, asset2);

			if balance1.is_zero() || balance2.is_zero() {
				Err(Error::<T>::PoolNotFound)?;
			}

			Ok((balance1, balance2))
		}

		/// Leading to an amount at the end of a `path`, get the required amounts in.
		pub(crate) fn balance_path_from_amount_out(
			amount_out: T::Unit,
			path: Vec<T::AssetKind>,
		) -> Result<BalancePath<T>, DispatchError> {
			let mut balance_path: BalancePath<T> = Vec::with_capacity(path.len());
			let mut amount_in: T::Unit = amount_out;

			let mut iter = path.into_iter().rev().peekable();
			while let Some(asset2) = iter.next() {
				let asset1 = match iter.peek() {
					Some(a) => a,
					None => {
						balance_path.push((asset2, amount_in));
						break
					},
				};
				let (reserve_in, reserve_out) = Self::get_reserves(asset1.clone(), asset2.clone())?;
				balance_path.push((asset2, amount_in));
				amount_in = Self::get_amount_in(&amount_in, &reserve_in, &reserve_out)?;
			}
			balance_path.reverse();

			Ok(balance_path)
		}

		/// Following an amount into a `path`, get the corresponding amounts out.
		pub(crate) fn balance_path_from_amount_in(
			amount_in: T::Unit,
			path: Vec<T::AssetKind>,
		) -> Result<BalancePath<T>, DispatchError> {
			let mut balance_path: BalancePath<T> = Vec::with_capacity(path.len());
			let mut amount_out: T::Unit = amount_in;

			let mut iter = path.into_iter().peekable();
			while let Some(asset1) = iter.next() {
				let asset2 = match iter.peek() {
					Some(a) => a,
					None => {
						balance_path.push((asset1, amount_out));
						break
					},
				};
				let (reserve_in, reserve_out) = Self::get_reserves(asset1.clone(), asset2.clone())?;
				balance_path.push((asset1, amount_out));
				amount_out = Self::get_amount_out(&amount_out, &reserve_in, &reserve_out)?;
			}
			Ok(balance_path)
		}

		/// Used by the RPC service to provide current prices.
		pub fn quote_price_exact_tokens_for_tokens(
			asset1: T::AssetKind,
			asset2: T::AssetKind,
			amount: T::Unit,
			include_fee: bool,
		) -> Option<T::Unit> {
			let pool_account = T::PoolLocator::pool_address(&asset1, &asset2).ok()?;

			let balance1 = Self::get_balance(&pool_account, asset1);
			let balance2 = Self::get_balance(&pool_account, asset2);
			if !balance1.is_zero() {
				if include_fee {
					Self::get_amount_out(&amount, &balance1, &balance2).ok()
				} else {
					Self::quote(&amount, &balance1, &balance2).ok()
				}
			} else {
				None
			}
		}

		/// Used by the RPC service to provide current prices.
		pub fn quote_price_tokens_for_exact_tokens(
			asset1: T::AssetKind,
			asset2: T::AssetKind,
			amount: T::Unit,
			include_fee: bool,
		) -> Option<T::Unit> {
			let pool_account = T::PoolLocator::pool_address(&asset1, &asset2).ok()?;

			let balance1 = Self::get_balance(&pool_account, asset1);
			let balance2 = Self::get_balance(&pool_account, asset2);
			if !balance1.is_zero() {
				if include_fee {
					Self::get_amount_in(&amount, &balance1, &balance2).ok()
				} else {
					Self::quote(&amount, &balance2, &balance1).ok()
				}
			} else {
				None
			}
		}

		/// Calculates the optimal amount from the reserves.
		pub fn quote(
			amount: &T::Unit,
			reserve1: &T::Unit,
			reserve2: &T::Unit,
		) -> Result<T::Unit, Error<T>> {
			// (amount * reserve2) / reserve1
			Self::mul_div(amount, reserve2, reserve1)
		}

		pub(super) fn calc_lp_amount_for_zero_supply(
			amount1: &T::Unit,
			amount2: &T::Unit,
		) -> Result<T::Unit, Error<T>> {
			let amount1 = T::HigherPrecisionUnit::from(*amount1);
			let amount2 = T::HigherPrecisionUnit::from(*amount2);

			let result = amount1
				.checked_mul(&amount2)
				.ok_or(Error::<T>::Overflow)?
				.integer_sqrt()
				.checked_sub(&T::MintMinLiquidity::get().into())
				.ok_or(Error::<T>::InsufficientLiquidityMinted)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		fn mul_div(a: &T::Unit, b: &T::Unit, c: &T::Unit) -> Result<T::Unit, Error<T>> {
			let a = T::HigherPrecisionUnit::from(*a);
			let b = T::HigherPrecisionUnit::from(*b);
			let c = T::HigherPrecisionUnit::from(*c);

			let result = a
				.checked_mul(&b)
				.ok_or(Error::<T>::Overflow)?
				.checked_div(&c)
				.ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Calculates amount out.
		///
		/// Given an input amount of an asset and pair reserves, returns the maximum output amount
		/// of the other asset.
		pub fn get_amount_out(
			amount_in: &T::Unit,
			reserve_in: &T::Unit,
			reserve_out: &T::Unit,
		) -> Result<T::Unit, Error<T>> {
			let amount_in = T::HigherPrecisionUnit::from(*amount_in);
			let reserve_in = T::HigherPrecisionUnit::from(*reserve_in);
			let reserve_out = T::HigherPrecisionUnit::from(*reserve_out);

			if reserve_in.is_zero() || reserve_out.is_zero() {
				return Err(Error::<T>::ZeroLiquidity)
			}

			let amount_in_with_fee = amount_in
				.checked_mul(&(T::HigherPrecisionUnit::from(1000u32) - (T::LPFee::get().into())))
				.ok_or(Error::<T>::Overflow)?;

			let numerator =
				amount_in_with_fee.checked_mul(&reserve_out).ok_or(Error::<T>::Overflow)?;

			let denominator = reserve_in
				.checked_mul(&1000u32.into())
				.ok_or(Error::<T>::Overflow)?
				.checked_add(&amount_in_with_fee)
				.ok_or(Error::<T>::Overflow)?;

			let result = numerator.checked_div(&denominator).ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Calculates amount in.
		///
		/// Given an output amount of an asset and pair reserves, returns a required input amount
		/// of the other asset.
		pub fn get_amount_in(
			amount_out: &T::Unit,
			reserve_in: &T::Unit,
			reserve_out: &T::Unit,
		) -> Result<T::Unit, Error<T>> {
			let amount_out = T::HigherPrecisionUnit::from(*amount_out);
			let reserve_in = T::HigherPrecisionUnit::from(*reserve_in);
			let reserve_out = T::HigherPrecisionUnit::from(*reserve_out);

			if reserve_in.is_zero() || reserve_out.is_zero() {
				Err(Error::<T>::ZeroLiquidity)?
			}

			if amount_out >= reserve_out {
				Err(Error::<T>::AmountOutTooHigh)?
			}

			let numerator = reserve_in
				.checked_mul(&amount_out)
				.ok_or(Error::<T>::Overflow)?
				.checked_mul(&1000u32.into())
				.ok_or(Error::<T>::Overflow)?;

			let denominator = reserve_out
				.checked_sub(&amount_out)
				.ok_or(Error::<T>::Overflow)?
				.checked_mul(&(T::HigherPrecisionUnit::from(1000u32) - T::LPFee::get().into()))
				.ok_or(Error::<T>::Overflow)?;

			let result = numerator
				.checked_div(&denominator)
				.ok_or(Error::<T>::Overflow)?
				.checked_add(&One::one())
				.ok_or(Error::<T>::Overflow)?;

			result.try_into().map_err(|_| Error::<T>::Overflow)
		}

		/// Ensure that a path is valid.
		fn validate_swap_path(path: &Vec<T::AssetKind>) -> Result<(), DispatchError> {
			ensure!(path.len() >= 2, Error::<T>::InvalidPath);
			ensure!(path.len() as u32 <= T::MaxSwapPathLength::get(), Error::<T>::InvalidPath);

			// validate all the pools in the path are unique
			let mut pools = BTreeSet::<T::PoolId>::new();
			for assets_pair in path.windows(2) {
				if let [asset1, asset2] = assets_pair {
					let pool_id = T::PoolLocator::pool_id(asset1, asset2)
						.map_err(|_| Error::<T>::InvalidAssetPair)?;

					let new_element = pools.insert(pool_id);
					if !new_element {
						return Err(Error::<T>::NonUniquePath.into())
					}
				}
			}
			Ok(())
		}

		/// Returns the next pool asset id for benchmark purposes only.
		#[cfg(any(test, feature = "runtime-benchmarks"))]
		pub fn get_next_pool_asset_id() -> T::PoolAssetId {
			NextPoolAssetId::<T>::get()
				.or(T::PoolAssetId::initial_value())
				.expect("Next pool asset ID can not be None")
		}
	}
}

sp_api::decl_runtime_apis! {
	/// This runtime api allows people to query the size of the liquidity pools
	/// and quote prices for swaps.
	pub trait AssetConversionApi<Balance, AssetId>
	where
		Balance: frame_support::traits::tokens::Balance + MaybeDisplay,
		AssetId: Codec,
	{
		/// Provides a quote for [`Pallet::swap_tokens_for_exact_tokens`].
		///
		/// Note that the price may have changed by the time the transaction is executed.
		/// (Use `amount_in_max` to control slippage.)
		fn quote_price_tokens_for_exact_tokens(
			asset1: AssetId,
			asset2: AssetId,
			amount: Balance,
			include_fee: bool,
		) -> Option<Balance>;

		/// Provides a quote for [`Pallet::swap_exact_tokens_for_tokens`].
		///
		/// Note that the price may have changed by the time the transaction is executed.
		/// (Use `amount_out_min` to control slippage.)
		fn quote_price_exact_tokens_for_tokens(
			asset1: AssetId,
			asset2: AssetId,
			amount: Balance,
			include_fee: bool,
		) -> Option<Balance>;

		/// Returns the size of the liquidity pool for the given asset pair.
		fn get_reserves(asset1: AssetId, asset2: AssetId) -> Option<(Balance, Balance)>;
	}
}

sp_core::generate_feature_enabled_macro!(runtime_benchmarks_enabled, feature = "runtime-benchmarks", $);
