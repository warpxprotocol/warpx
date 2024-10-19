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

fn events() -> Vec<Event<Test>> {
	let result = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let mock::RuntimeEvent::HybridOrderbook(inner) = e {
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

fn pool_with_default_liquidity(
	provider: MockAccountId,
	base: &NativeOrWithId<u32>,
	quote: &NativeOrWithId<u32>,
	order_quantity: u64,
	base_provided: MockBalance,
	quote_provided: MockBalance,
	tick_size: u64,
	lot_size: u64,
) {
	create_tokens(provider, vec![base.clone(), quote.clone()]);
	assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), provider, 1000));
	assert_ok!(HybridOrderbook::create_pool(
		RuntimeOrigin::signed(provider),
		Box::new(base.clone()),
		Box::new(quote.clone()),
		Permill::zero(),
		tick_size,
		lot_size
	));
	let ed = get_native_ed();
	assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), provider, 10000 * 2 + ed));
	assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, provider, base_provided * 10));
	assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 2, provider, quote_provided * 2));
	assert_ok!(HybridOrderbook::add_liquidity(
		RuntimeOrigin::signed(provider),
		Box::new(base.clone()),
		Box::new(quote.clone()),
		base_provided,
		quote_provided,
		base_provided,
		quote_provided,
		provider,
	));

	let pool_price = HybridOrderbook::pool_price(base, quote).unwrap();
	let mut order_price = pool_price - tick_size;
	// bid
	while order_price > 0 {
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(provider),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			true,
			order_price,
			order_quantity,
		));
		order_price -= tick_size;
	}

	// ask
	let mut order_price = pool_price + tick_size;
	let max_ask = pool_price * 2;
	while order_price <= max_ask {
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(provider),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			false,
			order_price,
			order_quantity,
		));

		order_price += tick_size;
	}
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
	new_test_ext().execute_with(|| {
		let user: MockAccountId = 1;
		let base_asset = NativeOrWithId::WithId(1);
		let quote_asset = NativeOrWithId::WithId(2);
		let pool_id = (base_asset.clone(), quote_asset.clone());
		create_tokens(user, vec![base_asset.clone(), quote_asset.clone()]);
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 1000));
		assert_ok!(HybridOrderbook::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(base_asset.clone()),
			Box::new(quote_asset.clone()),
			Permill::zero(),
			5,
			1
		));
		let Pool { lp_token, taker_fee_rate, tick_size, lot_size, bids, asks, .. } =
			Pools::<Test>::get(&pool_id).unwrap();
		assert_eq!(bids, CritbitTree::new());
		assert_eq!(asks, CritbitTree::new());
		assert_eq!(
			events(),
			[Event::<Test>::PoolCreated {
				creator: user,
				pool_id: pool_id.clone(),
				pool_account: <Test as Config>::PoolLocator::address(&pool_id).unwrap(),
				lp_token,
				taker_fee_rate,
				tick_size,
				lot_size,
			}]
		);
		assert_eq!(lp_token + 1, HybridOrderbook::get_next_pool_asset_id());
		assert_eq!(pools(), vec![pool_id]);
		assert_eq!(assets(), vec![base_asset.clone(), quote_asset.clone()]);
		assert_eq!(pool_assets(), vec![lp_token]);

		assert_noop!(
			HybridOrderbook::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(base_asset.clone()),
				Box::new(base_asset.clone()),
				Permill::zero(),
				5,
				1
			),
			Error::<Test>::InvalidAssetPair
		);
		assert_noop!(
			HybridOrderbook::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(quote_asset.clone()),
				Box::new(quote_asset.clone()),
				Permill::zero(),
				5,
				1
			),
			Error::<Test>::InvalidAssetPair
		);
	});
}

#[test]
fn add_liquidity_works() {
	new_test_ext().execute_with(|| {
		let user: MockAccountId = 1;
		let base = NativeOrWithId::WithId(1);
		let quote = NativeOrWithId::WithId(2);
		let pool_id = (base.clone(), quote.clone());
		create_tokens(user, vec![base.clone(), quote.clone()]);
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 1000));
		let lp_token1 = HybridOrderbook::get_next_pool_asset_id();
		assert_ok!(HybridOrderbook::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			Permill::zero(),
			5,
			1
		));
		let ed = get_native_ed();
		let base_provided = 100;
		let quote_provided = 100000;
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 10000 * 2 + ed));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 1, user, base_provided * 10));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user, quote_provided * 2));
		assert_ok!(HybridOrderbook::add_liquidity(
			RuntimeOrigin::signed(user),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			base_provided,
			quote_provided,
			100,
			100000,
			user,
		));
		assert!(events().contains(&Event::<Test>::LiquidityAdded {
			who: user,
			mint_to: user,
			pool_id: pool_id.clone(),
			base_asset_provided: base_provided,
			quote_asset_provided: quote_provided,
			lp_token: lp_token1,
			lp_token_minted: 3062,
		}));
		let pallet_account = <Test as Config>::PoolLocator::address(&pool_id).unwrap();
		assert_eq!(balance(pallet_account, base.clone()), base_provided);
		assert_eq!(balance(pallet_account, quote.clone()), quote_provided);
		assert_eq!(balance(user, base.clone()), base_provided * 9);
		assert_eq!(balance(user, quote.clone()), quote_provided);
		assert_eq!(pool_balance(user, lp_token1), 3062);
		let reserves = HybridOrderbook::get_reserves(&base, &quote).unwrap();
		assert_eq!(reserves, (base_provided, quote_provided));
		let pool_price = HybridOrderbook::pool_price(&base, &quote).unwrap();
		assert_eq!(pool_price, 1000); // meaning base:quote=1:1000
	})
}

#[test]
fn limit_order_works() {
	new_test_ext().execute_with(|| {
		let user: MockAccountId = 1;
		let user2: MockAccountId = 2;
		let base = NativeOrWithId::WithId(1);
		let quote = NativeOrWithId::WithId(2);
		let pool_id = (base.clone(), quote.clone());
		create_tokens(user, vec![base.clone(), quote.clone()]);
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 1000));
		let lp_token1 = HybridOrderbook::get_next_pool_asset_id();
		let tick_size = 5;
		let lot_size = 1;
		assert_ok!(HybridOrderbook::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			Permill::zero(),
			tick_size,
			lot_size
		));
		let ed = get_native_ed();
		let base_provided = 10000;
		let quote_provided = 10000000;
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 10000 * 2 + ed));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 1, user, base_provided * 10000));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user, quote_provided * 100));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user2, 2000000));
		println!("{:?}", Assets::balance(2, &user2));
		// Liquidity should be added first
		assert_noop!(
			HybridOrderbook::limit_order(
				RuntimeOrigin::signed(user),
				Box::new(base.clone()),
				Box::new(quote.clone()),
				true,
				5,
				100
			),
			Error::<Test>::ZeroLiquidity
		);
		assert_ok!(HybridOrderbook::add_liquidity(
			RuntimeOrigin::signed(user),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			base_provided,
			quote_provided,
			base_provided,
			quote_provided,
			user,
		));
		let pool_price = HybridOrderbook::pool_price(&base, &quote).unwrap();
		let Pool { tick_size, .. } = Pools::<Test>::get(&pool_id).unwrap();
		// order price should be multiple of tick
		assert_noop!(
			HybridOrderbook::limit_order(
				RuntimeOrigin::signed(user),
				Box::new(base.clone()),
				Box::new(quote.clone()),
				true,
				2,
				100
			),
			Error::<Test>::InvalidOrderPrice
		);
		let mut order_price = pool_price - tick_size;
		let order_quantity = 100;
		let is_bid = true;
		// bid
		while order_price > 0 {
			assert_ok!(HybridOrderbook::limit_order(
				RuntimeOrigin::signed(user),
				Box::new(base.clone()),
				Box::new(quote.clone()),
				is_bid,
				order_price,
				order_quantity,
			));
			assert!(events().contains(&Event::<Test>::LimitOrder {
				pool_id: pool_id.clone(),
				maker: user,
				order_price,
				order_quantity,
				is_bid,
			}));
			order_price -= tick_size;
		}
		// ask
		let mut order_price = pool_price + tick_size;
		let order_quantity = 50;
		let max_ask = pool_price * 2;
		while order_price <= max_ask {
			assert_ok!(HybridOrderbook::limit_order(
				RuntimeOrigin::signed(user),
				Box::new(base.clone()),
				Box::new(quote.clone()),
				!is_bid,
				order_price,
				order_quantity,
			));
			assert!(events().contains(&Event::<Test>::LimitOrder {
				pool_id: pool_id.clone(),
				maker: user,
				order_price,
				order_quantity,
				is_bid: !is_bid,
			}));
			order_price += tick_size;
		}

		assert_ok!(HybridOrderbook::market_order(
			RuntimeOrigin::signed(user2),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			150,
			true,
		));
		let balance1 = Assets::balance(1, &user);
		let balance2 = Assets::balance(2, &user);
		let user2_balance1 = Assets::balance(1, &user2);
		println!("Balance 1 => {:?}, Balance 2 => {:?}", balance1, balance2);
		// No fees for buying yet
		assert_eq!(user2_balance1, 150);
	})
}

#[test]
fn market_order_works() {
	new_test_ext().execute_with(|| {
		let initial_provider: MockAccountId = 1;
		let base = NativeOrWithId::WithId(1);
		let quote = NativeOrWithId::WithId(2);
		let pool_id = (base.clone(), quote.clone());
		let order_quantity = 50;
		let base_provided = 1000;
		let quote_provided = 100000;
		let tick_size = 1;
		let lot_size = 1;
		pool_with_default_liquidity(
			initial_provider,
			&base,
			&quote,
			order_quantity,
			base_provided,
			quote_provided,
			tick_size,
			lot_size,
		);
		let pool = Pools::<Test>::get(&pool_id).unwrap();
		assert!(pool.asks.size() == 100);
		assert!(pool.bids.size() == 99);
		let user2: MockAccountId = 2;
		let user3: MockAccountId = 3;
		let user4: MockAccountId = 4;
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(user2),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			false,
			101,
			100
		));
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(user3),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			false,
			101,
			200
		));
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(user4),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			false,
			101,
			300
		));
		let pool = Pools::<Test>::get(&pool_id).unwrap();
		println!("Before => {:?}", pool.asks);
		assert_ok!(HybridOrderbook::market_order(
			RuntimeOrigin::signed(initial_provider),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			450,
			true,
		));
		let pool = Pools::<Test>::get(&pool_id).unwrap();
		println!("After => {:?}", pool.asks);
	})
}

#[test]
fn cancel_order_works() {
	new_test_ext().execute_with(|| {
		let initial_provider: MockAccountId = 1;
		let base = NativeOrWithId::WithId(1);
		let quote = NativeOrWithId::WithId(2);
		let pool_id = (base.clone(), quote.clone());
		let order_quantity = 50;
		// Default pool price => 100
		let base_provided = 1000;
		let quote_provided = 100000;
		let tick_size = 1;
		let lot_size = 1;
		pool_with_default_liquidity(
			initial_provider,
			&base,
			&quote,
			order_quantity,
			base_provided,
			quote_provided,
			tick_size,
			lot_size,
		);
		let order_price = 100 + tick_size;
		assert_ok!(HybridOrderbook::limit_order(
			RuntimeOrigin::signed(2),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			false,
			order_price,
			50
		));
		assert_ok!(HybridOrderbook::cancel_order(
			RuntimeOrigin::signed(2),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			order_price,
			9223372036854775908.into(),
			10
		));
		// Only owner can cancel
		assert_noop!(
			HybridOrderbook::cancel_order(
				RuntimeOrigin::signed(1),
				Box::new(base.clone()),
				Box::new(quote.clone()),
				order_price,
				9223372036854775908.into(),
				10
			),
			Error::<Test>::ErrorOnCancelOrder
		);

		// Cannot cancel more than existed
		assert_ok!(HybridOrderbook::cancel_order(
			RuntimeOrigin::signed(2),
			Box::new(base.clone()),
			Box::new(quote.clone()),
			order_price,
			9223372036854775908.into(),
			40,
		));
		let pool = Pools::<Test>::get(&pool_id).unwrap();
		assert!(pool.orders_for(&2, false).is_empty());
	})
}
