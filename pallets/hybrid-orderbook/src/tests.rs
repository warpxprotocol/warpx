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

use crate::{
	mock::{AccountId as MockAccountId, Balance as MockBalance, *},
	*,
};
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop,
	instances::Instance1,
	traits::{
		fungible,
		fungible::{Inspect as FungibleInspect, NativeOrWithId},
		fungibles,
		fungibles::{Inspect, InspectEnumerable},
		Get,
	},
};
use sp_arithmetic::Permill;
use sp_runtime::{DispatchError, TokenError};

fn events() -> Vec<Event<Test>> {
	let result = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let mock::RuntimeEvent::AssetConversion(inner) = e {
				Some(inner)
			} else {
				None
			}
		})
		.collect();

	System::reset_events();

	result
}

fn pools() -> Vec<<Test as Config>::PoolId> {
	let mut s: Vec<_> = Pools::<Test>::iter().map(|x| x.0).collect();
	s.sort();
	s
}

fn assets() -> Vec<NativeOrWithId<u32>> {
	let mut s: Vec<_> = Assets::asset_ids().map(|id| NativeOrWithId::WithId(id)).collect();
	s.sort();
	s
}

fn pool_assets() -> Vec<u32> {
	let mut s: Vec<_> = <<Test as Config>::PoolAssets>::asset_ids().collect();
	s.sort();
	s
}

fn create_tokens(owner: MockAccountId, tokens: Vec<NativeOrWithId<u32>>) {
	create_tokens_with_ed(owner, tokens, 1)
}

fn create_tokens_with_ed(owner: MockAccountId, tokens: Vec<NativeOrWithId<u32>>, ed: MockBalance) {
	for token_id in tokens {
		let asset_id = match token_id {
			NativeOrWithId::WithId(id) => id,
			_ => unreachable!("invalid token"),
		};
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, false, ed));
	}
}

fn balance(owner: MockAccountId, token_id: NativeOrWithId<u32>) -> MockBalance {
	<<Test as Config>::Assets>::balance(token_id, &owner)
}

fn pool_balance(owner: MockAccountId, token_id: u32) -> MockBalance {
	<<Test as Config>::PoolAssets>::balance(token_id, owner)
}

fn get_native_ed() -> MockBalance {
	<<Test as Config>::Assets>::minimum_balance(NativeOrWithId::Native)
}

macro_rules! bvec {
	($($x:expr),+ $(,)?) => (
		vec![$( Box::new( $x ), )*]
	)
}

// #[test]
// fn check_max_numbers() {
// 	new_test_ext().execute_with(|| {
// 		assert_eq!(AssetConversion::quote(&3u128, &u128::MAX, &u128::MAX).ok().unwrap(), 3);
// 		assert!(AssetConversion::quote(&u128::MAX, &3u128, &u128::MAX).is_err());
// 		assert_eq!(AssetConversion::quote(&u128::MAX, &u128::MAX, &1u128).ok().unwrap(), 1);

// 		assert_eq!(
// 			AssetConversion::get_amount_out(&100u128, &u128::MAX, &u128::MAX).ok().unwrap(),
// 			99
// 		);
// 		assert_eq!(
// 			AssetConversion::get_amount_in(&100u128, &u128::MAX, &u128::MAX).ok().unwrap(),
// 			101
// 		);
// 	});
// }

#[test]
fn create_pool_works() {
	new_test_ext().execute_with(|| {});
}
