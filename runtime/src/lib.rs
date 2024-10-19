#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

use alloc::{vec, vec::Vec};
use smallvec::smallvec;

use polkadot_sdk::{staging_parachain_info as parachain_info, *};

mod apis;
use apis::*;
mod configs;
mod constants;
pub use constants::{currency::*, time::*};

// Primtivies
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, U256};
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		AccountIdConversion, BlakeTwo256, Block as BlockT, IdentifyAccount, NumberFor, One, Verify,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, MultiSignature,
};
pub use sp_runtime::{Perbill, Permill};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// System
pub use frame_support::{
	construct_runtime, derive_impl,
	instances::{Instance1, Instance2},
	ord_parameter_types, parameter_types,
	traits::{
		fungible::{NativeFromLeft, NativeOrWithId, UnionOf},
		tokens::imbalance::ResolveAssetTo,
		AsEnsureOriginWithArg, ConstBool, ConstU128, ConstU32, ConstU64, ConstU8,
		KeyOwnerProofSystem, Randomness, StorageInfo,
	},
	weights::{
		constants::{
			BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND,
		},
		IdentityFee, Weight, WeightToFeeCoefficient, WeightToFeeCoefficients,
		WeightToFeePolynomial,
	},
	PalletId, StorageValue,
};
pub use frame_system::Call as SystemCall;
use frame_system::{EnsureRoot, EnsureSigned, EnsureSignedBy};

// FRAMES
pub use pallet_balances::Call as BalancesCall;
use pallet_hybrid_orderbook::{
	Ascending, BaseQuoteAsset, Chain, CritbitTree, Pool, Tick, WithFirstAsset,
};
pub use pallet_timestamp::Call as TimestampCall;
use pallet_transaction_payment::{ConstFeeMultiplier, CurrencyAdapter, Multiplier};
use polkadot_runtime_common::SlowAdjustingFeeUpdate;

pub use primitives::*;
pub mod primitives {

	use super::*;
	/// An index to a block.
	pub type BlockNumber = u32;

	/// Type used for expressing timestamp.
	pub type Moment = u64;

	/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
	pub type Signature = MultiSignature;

	/// Some way of identifying an account on the chain. We intentionally make it equivalent
	/// to the public key of our transaction signing scheme.
	pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

	/// Balance of an account.
	pub type Balance = u64;

	/// Index of a transaction in the chain.
	pub type Nonce = u32;

	/// A hash of some data used by the chain.
	pub type Hash = sp_core::H256;

	/// Identifier of each asset.
	pub type AssetId = u32;

	/// Index of each order in the orderbook which is used as the key in the orderbook.
	pub type OrderBookIndex = u64;

	/// Identifier of each pool
	pub type PoolId = u64;
}

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;

	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;

	impl_opaque_keys! {
		pub struct SessionKeys {
			pub aura: Aura,
		}
	}
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("warp(x)"),
	impl_name: create_runtime_str!("warp(x)"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 1,
	apis: apis::RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 1,
};

#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Runtime;

	#[runtime::pallet_index(0)]
	type System = frame_system;

	#[runtime::pallet_index(1)]
	type Timestamp = pallet_timestamp;

	#[runtime::pallet_index(5)]
	type Balances = pallet_balances;

	#[runtime::pallet_index(20)]
	type Aura = pallet_aura;

	#[runtime::pallet_index(30)]
	type TransactionPayment = pallet_transaction_payment;

	#[runtime::pallet_index(38)]
	pub type Assets = pallet_assets<Instance1>;

	#[runtime::pallet_index(39)]
	pub type PoolAssets = pallet_assets<Instance2>;

	#[runtime::pallet_index(50)]
	pub type HybridOrderbook = pallet_hybrid_orderbook;

	#[runtime::pallet_index(99)]
	type Sudo = pallet_sudo;
}

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
type Migrations = ();

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
>;

/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
/// node's balance type.
///
/// This should typically create a mapping between the following ranges:
///   - `[0, MAXIMUM_BLOCK_WEIGHT]`
///   - `[Balance::min, Balance::max]`
///
/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
///   - Setting it to `0` will essentially disable the weight fee.
///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		// in Rococo, extrinsic base weight (smallest non-zero weight) is mapped to 1 MILLI_UNIT:
		// in our template, we map to 1/10 of that, or 1/10 MILLI_UNIT
		let p = CENTS / 10;
		let q = 100 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
		smallvec![WeightToFeeCoefficient {
			degree: 1,
			negative: false,
			coeff_frac: Perbill::from_rational(p % q, q),
			coeff_integer: p / q,
		}]
	}
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_timestamp, Timestamp]
		[pallet_sudo, Sudo]
		[pallet_template, TemplateModule]
	);
}
