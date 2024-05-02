#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

// #[cfg(test)]
// mod mock;

// #[cfg(test)]
// mod test;

pub use codec::{Decode, Encode, MaxEncodedLen};
pub use scale_info::TypeInfo;

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
	PalletId,
};
use sp_core::Get;
use sp_runtime::{
	traits::{
		CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Ensure, IntegerSquareRoot, MaybeDisplay,
		One, TrailingZeroInput, Zero,
	},
	DispatchError, Saturating, TokenError, TransactionOutcome,
};
use sp_std::collections::btree_map::BTreeMap;
use sp_arithmetic::{traits:: Unsigned, Permill};

pub use pallet::*;

mod types;
use types::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type in which the assets for swapping are measured.
		type Balance: Balance;

		/// A type used for calculations concerning the `Balance` type to avoid possible overflows.
		type HigherPrecisionBalance: IntegerSquareRoot
			+ One
			+ Ensure
			+ Unsigned
			+ From<u32>
			+ From<Self::Balance>
			+ TryInto<Self::Balance>;

		/// Type of asset class, sourced from [`Config::Assets`], utilized to offer liquidity to a
		/// pool.
		type AssetKind: Parameter + MaxEncodedLen;

		/// Registry of assets utilized for providing liquidity to pools.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetKind, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ AccountTouch<Self::AssetKind, Self::AccountId, Balance = Self::Balance>
			+ Balanced<Self::AccountId>;

		/// Liquidity pool identifier.
		type PoolId: Parameter + MaxEncodedLen + Ord;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	#[pallet::getter(fn markets)]
	#[pallet::unbounded]
	pub type Pools<T: Config> = StorageMap<_, 
		Twox64Concat, 
		T::PoolId, 
		Pool<T::AssetKind, T::AccountId, BlockNumberFor<T>, T::Balance, u64>
	>;

	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/main-docs/build/events-errors/
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		SomethingStored { something: u32, who: T::AccountId },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn do_something(origin: OriginFor<T>, something: u32) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// https://docs.substrate.io/main-docs/build/origins/
			let who = ensure_signed(origin)?;

			// Emit an event.
			Self::deposit_event(Event::SomethingStored { something, who });
			// Return a successful DispatchResultWithPostInfo
			Ok(())
		}
	}
}
