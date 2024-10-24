mod xcm_config;

// Local module imports
use super::{
	parachain_info, vec, AccountId, AccountIdConversion, AsEnsureOriginWithArg, AssetId, Assets,
	Aura, AuraId, Balance, Balances, BaseQuoteAsset, Block, BlockNumber, CollatorSelection,
	ConsensusHook, CritbitTree, EnsureSigned, EnsureSignedBy, Hash, HybridOrderbook, Instance1,
	Instance2, MessageQueue, NativeFromLeft, NativeOrWithId, Nonce, PalletInfo, ParachainSystem,
	Perbill, Permill, PoolAssets, ResolveAssetTo, RocksDbWeight, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask, Session,
	SessionKeys, Tick, UnionOf, WeightToFee, XcmpQueue, DAYS, DOLLARS, HOURS, MAXIMUM_BLOCK_WEIGHT,
	MILLICENTS, NORMAL_DISPATCH_RATIO, SLOT_DURATION, VERSION, WEIGHT_REF_TIME_PER_SECOND,
};
use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstBool, ConstU128, ConstU32, ConstU64, ConstU8, EitherOfDiverse, TransformOrigin},
	weights::{ConstantMultiplier, Weight},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot,
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use parachains_common::message_queue::{NarrowOriginToSibling, ParaIdToSibling};
use polkadot_runtime_common::{
	xcm_sender::NoPriceForMessageDelivery, BlockHashCount, SlowAdjustingFeeUpdate,
};
use polkadot_sdk::{
	staging_xcm as xcm, staging_xcm_builder as xcm_builder, staging_xcm_executor as xcm_executor, *,
};
use sp_version::RuntimeVersion;
use xcm::latest::prelude::BodyId;
use xcm_config::{RelayLocation, XcmOriginToTransactDispatchOrigin};

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights =
		BlockWeights::with_sensible_defaults(
			Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
	pub RuntimeBlockLength: BlockLength = BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

// impls

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The index type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The block type.
	type Block = Block;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Runtime version.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The action to take on a Runtime Upgrade
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const Period: u32 = 6 * HOURS;
	pub const Offset: u32 = 0;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	// we don't have stash and controller, thus we don't need the convert as well.
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = CollatorSelection;
	// Essentially just Aura, but let's be pedantic.
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type WeightInfo = ();
}

#[docify::export(aura_config)]
impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<100_000>;
	type AllowMultipleBlocksPerSlot = ConstBool<true>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type EventHandler = (CollatorSelection,);
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1 * DOLLARS;
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = frame_system::Pallet<Runtime>;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = ConstU32<1>;
}

parameter_types! {
	pub const TransactionByteFee: Balance = 1 * MILLICENTS;
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
}

parameter_types! {
	pub const AssetDeposit: Balance = 100 * DOLLARS;
	pub const ApprovalDeposit: Balance = 1 * DOLLARS;
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 10 * DOLLARS;
	pub const MetadataDepositPerByte: Balance = 1 * DOLLARS;
}

impl pallet_assets::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = AssetId;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = ConstU128<DOLLARS>;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = HybridOrderbook;
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
	type RemoveItemsLimit = ConstU32<1000>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

ord_parameter_types! {
	pub const HybridOrderBookOrigin: AccountId = AccountIdConversion::<AccountId>::into_account_truncating(&HybridOrderBookPalletId::get());
}

impl pallet_assets::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = AssetId;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSignedBy<HybridOrderBookOrigin, AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = ConstU128<DOLLARS>;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub const HybridOrderBookPalletId: PalletId = PalletId(*b"hyammord");
	pub const PoolSetupFee: Balance = 1 * DOLLARS; // should be more or equal to the existential deposit
	pub const MintMinLiquidity: Balance = 100;  // 100 is good enough when the main currency has 10-12 decimals.
	pub const LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
	pub const Native: NativeOrWithId<u32> = NativeOrWithId::Native;
	pub const OrderExpiration: BlockNumber = DAYS;
}

impl pallet_hybrid_orderbook::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Unit = Balance;
	type HigherPrecisionUnit = u128;
	type AssetKind = NativeOrWithId<u32>;
	type Assets = UnionOf<Balances, Assets, NativeFromLeft, NativeOrWithId<u32>, AccountId>;
	type OrderBook = CritbitTree<Balance, Tick<Balance, AccountId, BlockNumber>>;
	type PoolId = (Self::AssetKind, Self::AssetKind);
	type PoolLocator = BaseQuoteAsset<AccountId, NativeOrWithId<u32>>;
	type PoolAssetId = <Self as pallet_assets::Config<Instance2>>::AssetId;
	type PoolAssets = PoolAssets;
	type PoolSetupFee = PoolSetupFee;
	type PoolSetupFeeAsset = Native;
	type PoolSetupFeeTarget = ResolveAssetTo<HybridOrderBookOrigin, Self::Assets>;
	type PalletId = HybridOrderBookPalletId;
	type OrderExpiration = OrderExpiration;
	type LPFee = ConstU32<3>; // means 0.3%
	type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
	type WeightInfo = pallet_hybrid_orderbook::weights::SubstrateWeight<Runtime>;
	type MaxSwapPathLength = ConstU32<4>;
	type MintMinLiquidity = MintMinLiquidity;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type OutboundXcmpMessageSource = XcmpQueue;
	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
	type ReservedDmpWeight = ReservedDmpWeight;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
	type ConsensusHook = ConsensusHook;
}

impl parachain_info::Config for Runtime {}

parameter_types! {
	pub MessageQueueServiceWeight: Weight = Perbill::from_percent(35) * RuntimeBlockWeights::get().max_block;
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor = pallet_message_queue::mock_helpers::NoopMessageProcessor<
		cumulus_primitives_core::AggregateMessageOrigin,
	>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = xcm_builder::ProcessXcmMessage<
		AggregateMessageOrigin,
		xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
		RuntimeCall,
	>;
	type Size = u32;
	// The XCMP queue pallet is only ever able to handle the `Sibling(ParaId)` origin:
	type QueueChangeHandler = NarrowOriginToSibling<XcmpQueue>;
	type QueuePausedQuery = NarrowOriginToSibling<XcmpQueue>;
	type HeapSize = sp_core::ConstU32<{ 103 * 1024 }>;
	type MaxStale = sp_core::ConstU32<8>;
	type ServiceWeight = MessageQueueServiceWeight;
	type IdleMaxServiceWeight = ();
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	// Enqueue XCMP messages from siblings for later processing.
	type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
	type MaxInboundSuspended = sp_core::ConstU32<1_000>;
	type MaxActiveOutboundChannels = ConstU32<128>;
	type MaxPageSize = ConstU32<{ 1 << 16 }>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = ();
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"PotStake");
	pub const SessionLength: BlockNumber = 6 * HOURS;
	// StakingAdmin pluralistic body.
	pub const StakingAdminBodyId: BodyId = BodyId::Defense;
}

/// We allow root and the StakingAdmin to execute privileged collator selection operations.
pub type CollatorSelectionUpdateOrigin = EitherOfDiverse<
	EnsureRoot<AccountId>,
	EnsureXcm<IsVoiceOfBody<RelayLocation, StakingAdminBodyId>>,
>;

impl pallet_collator_selection::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type UpdateOrigin = CollatorSelectionUpdateOrigin;
	type PotId = PotId;
	type MaxCandidates = ConstU32<100>;
	type MinEligibleCollators = ConstU32<4>;
	type MaxInvulnerables = ConstU32<20>;
	// should be a multiple of session or things will get inconsistent
	type KickThreshold = Period;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ValidatorRegistration = Session;
	type WeightInfo = ();
}
