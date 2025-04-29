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

//! Test environment for Asset Conversion pallet.

use super::*;
use crate as pallet_hybrid_orderbook;
use core::default::Default;
use frame_support::{
    construct_runtime, derive_impl,
    instances::{Instance1, Instance2},
    ord_parameter_types, parameter_types,
    traits::{
        tokens::{
            fungible::{NativeFromLeft, NativeOrWithId, UnionOf},
            imbalance::ResolveAssetTo,
        },
        AsEnsureOriginWithArg, ConstU32,
    },
    PalletId,
};
use frame_system::{EnsureSigned, EnsureSignedBy};
use sp_arithmetic::Permill;
use sp_core::{ConstU64, ConstU8};
use sp_runtime::{
    traits::{AccountIdConversion, IdentityLookup},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u128;
pub type Balance = u64;

construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets::<Instance1>,
        PoolAssets: pallet_assets::<Instance2>,
        AssetsFreezer: pallet_assets_freezer::<Instance1>,
        HybridOrderbook: pallet_hybrid_orderbook,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ConstU64<100>;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ();
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

impl pallet_assets::Config<Instance1> for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = ConstU32<1000>;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<Self::AccountId>>;
    type ForceOrigin = frame_system::EnsureRoot<Self::AccountId>;
    type AssetDeposit = ConstU64<1>;
    type AssetAccountDeposit = ConstU64<10>;
    type MetadataDepositBase = ConstU64<1>;
    type MetadataDepositPerByte = ConstU64<1>;
    type ApprovalDeposit = ConstU64<1>;
    type StringLimit = ConstU32<50>;
    type Holder = ();
    type Freezer = AssetsFreezer;
    type Extra = ();
    type WeightInfo = ();
    type CallbackHandle = ();
    pallet_assets::runtime_benchmarks_enabled! {
        type BenchmarkHelper = ();
    }
}

impl pallet_assets::Config<Instance2> for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = ConstU32<1000>;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin =
        AsEnsureOriginWithArg<EnsureSignedBy<HybridOrderbookOrigin, Self::AccountId>>;
    type ForceOrigin = frame_system::EnsureRoot<Self::AccountId>;
    type AssetDeposit = ConstU64<0>;
    type AssetAccountDeposit = ConstU64<0>;
    type MetadataDepositBase = ConstU64<0>;
    type MetadataDepositPerByte = ConstU64<0>;
    type ApprovalDeposit = ConstU64<0>;
    type StringLimit = ConstU32<50>;
    type Holder = ();
    type Freezer = ();
    type Extra = ();
    type WeightInfo = ();
    type CallbackHandle = ();
    pallet_assets::runtime_benchmarks_enabled! {
        type BenchmarkHelper = ();
    }
}

impl pallet_assets_freezer::Config<Instance1> for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeFreezeReason = RuntimeFreezeReason;
}

parameter_types! {
    pub const HybridOrderbookPalletId: PalletId = PalletId(*b"py/hybob");
    pub const Native: NativeOrWithId<u32> = NativeOrWithId::Native;
    pub storage LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
}

ord_parameter_types! {
    pub const HybridOrderbookOrigin: u128 = AccountIdConversion::<u128>::into_account_truncating(&HybridOrderbookPalletId::get());
}

pub type NativeAndAssets =
    UnionOf<Balances, Assets, NativeFromLeft, NativeOrWithId<u32>, AccountId>;
pub type NativeAndAssetsFreezer =
    UnionOf<Balances, AssetsFreezer, NativeFromLeft, NativeOrWithId<u32>, AccountId>;
pub type PoolIdToAccountId =
    AccountIdConverter<HybridOrderbookPalletId, (NativeOrWithId<u32>, NativeOrWithId<u32>)>;
pub type OrderbookLocator = BaseQuoteAsset<AccountId, NativeOrWithId<u32>>;

parameter_types! {
    pub const OrderExpiration: u64 = 100;
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Unit = <Self as pallet_balances::Config>::Balance;
    type HigherPrecisionUnit = u128;
    type AssetKind = NativeOrWithId<u32>;
    type Assets = NativeAndAssets;
    type AssetsFreezer = NativeAndAssetsFreezer;
    type OrderBook = CritbitTree<Balance, Tick<Balance, AccountId, u64>>;
    type OrderExpiration = OrderExpiration;
    type PoolId = (Self::AssetKind, Self::AssetKind);
    type PoolLocator = OrderbookLocator;
    type PoolAssetId = u32;
    type PoolAssets = PoolAssets;
    type PoolSetupFee = ConstU64<100>; // should be more or equal to the existential deposit
    type PoolSetupFeeAsset = Native;
    type PoolSetupFeeTarget = ResolveAssetTo<HybridOrderbookOrigin, Self::Assets>;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type PalletId = HybridOrderbookPalletId;
    type WeightInfo = ();
    type LPFee = ConstU32<3>; // means 0.3%
    type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
    type StandardDecimals = ConstU8<10>;
    type MaxSwapPathLength = ConstU32<4>;
    type MintMinLiquidity = ConstU64<100>; // 100 is good enough when the main currency has 12 decimals.
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 10000), (2, 20000), (3, 30000), (4, 40000)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
