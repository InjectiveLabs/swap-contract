use std::str::FromStr;

use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{coin, Addr};

use crate::admin::set_route;
use crate::contract::instantiate;
use injective_cosmwasm::{OwnedDepsExt, TEST_MARKET_ID_1, TEST_MARKET_ID_2};
use injective_math::FPDecimal;

use crate::msg::{FeeRecipient, InstantiateMsg};
use crate::queries::{estimate_swap_result, SwapQuantityMode};
use crate::state::get_all_swap_routes;
use crate::testing::test_utils::{
    mock_deps_eth_inj, round_usd_like_fee, MultiplierQueryBehavior, TEST_USER_ADDR,
};
use crate::types::{FPCoin, SwapRoute};

/// In this test we swap 1000 INJ to ETH, we assume avg price of INJ at 8 usdt and avg price of eth 2000 usdt
#[test]
fn test_calculate_swap_price() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let admin = &Addr::unchecked(TEST_USER_ADDR);

    instantiate(
        deps.as_mut_deps(),
        mock_env(),
        mock_info(admin.as_ref(), &[coin(1_000u128, "usdt")]),
        InstantiateMsg {
            fee_recipient: FeeRecipient::Address(admin.to_owned()),
            admin: admin.to_owned(),
        },
    )
    .unwrap();
    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "eth".to_string(),
        "inj".to_string(),
        vec![TEST_MARKET_ID_1.into(), TEST_MARKET_ID_2.into()],
    )
    .unwrap();

    let actual_swap_result = estimate_swap_result(
        deps.as_ref(),
        mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantityMode::InputQuantity(FPDecimal::from_str("12").unwrap()),
    )
    .unwrap();

    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("2879.74"),
        "Wrong amount of swap execution estimate received"
    ); // value rounded to min tick

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee denoms received"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("9368.749003"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("9444"),
        denom: "usdt".to_string(),
    };

    assert_eq!(
        round_usd_like_fee(
            &actual_swap_result.expected_fees[0],
            FPDecimal::must_from_str("0.000001")
        ),
        expected_fee_2,
        "Wrong amount of first fee received"
    );

    assert_eq!(
        round_usd_like_fee(
            &actual_swap_result.expected_fees[1],
            FPDecimal::must_from_str("0.000001")
        ),
        expected_fee_1,
        "Wrong amount of second fee received"
    );
}

/// In this test we swap 1000 INJ to ETH, we assume avg price of INJ at 8 usdt and avg price of eth 2000 usdt
#[test]
fn test_calculate_swap_price_self_relaying() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let admin = &Addr::unchecked(TEST_USER_ADDR);

    instantiate(
        deps.as_mut_deps(),
        mock_env(),
        mock_info(admin.as_ref(), &[coin(1_000u128, "usdt")]),
        InstantiateMsg {
            fee_recipient: FeeRecipient::SwapContract,
            admin: admin.to_owned(),
        },
    )
    .unwrap();

    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "eth".to_string(),
        "inj".to_string(),
        vec![TEST_MARKET_ID_1.into(), TEST_MARKET_ID_2.into()],
    )
    .unwrap();

    let actual_swap_result = estimate_swap_result(
        deps.as_ref(),
        mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantityMode::InputQuantity(FPDecimal::from_str("12").unwrap()),
    )
    .unwrap();

    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("2888.78"),
        "Wrong amount of swap execution estimate received"
    ); // value rounded to min tick

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee denoms received"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("5666.4"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("5639.2664"),
        denom: "usdt".to_string(),
    };

    assert_eq!(
        round_usd_like_fee(
            &actual_swap_result.expected_fees[0],
            FPDecimal::must_from_str("0.000001")
        ),
        expected_fee_1,
        "Wrong amount of fee received"
    );

    assert_eq!(
        round_usd_like_fee(
            &actual_swap_result.expected_fees[1],
            FPDecimal::must_from_str("0.000001")
        ),
        expected_fee_2,
        "Wrong amount of fee received"
    )
}

#[test]
fn get_all_queries_returns_empty_array_if_no_routes_are_set() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let admin = &Addr::unchecked(TEST_USER_ADDR);

    instantiate(
        deps.as_mut_deps(),
        mock_env(),
        mock_info(admin.as_ref(), &[coin(1_000u128, "usdt")]),
        InstantiateMsg {
            fee_recipient: FeeRecipient::SwapContract,
            admin: admin.to_owned(),
        },
    )
    .unwrap();

    let all_routes_result = get_all_swap_routes(deps.as_ref().storage);

    assert!(all_routes_result.is_ok(), "Error getting all routes");
    assert!(
        all_routes_result.unwrap().is_empty(),
        "Routes should be empty"
    );
}

#[test]
fn get_all_queries_returns_expected_array_if_routes_are_set() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let admin = &Addr::unchecked(TEST_USER_ADDR);

    instantiate(
        deps.as_mut_deps(),
        mock_env(),
        mock_info(admin.as_ref(), &[coin(1_000u128, "usdt")]),
        InstantiateMsg {
            fee_recipient: FeeRecipient::SwapContract,
            admin: admin.to_owned(),
        },
    )
    .unwrap();

    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "eth".to_string(),
        "inj".to_string(),
        vec![TEST_MARKET_ID_1.into(), TEST_MARKET_ID_2.into()],
    )
    .unwrap();

    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "eth".to_string(),
        "usdt".to_string(),
        vec![TEST_MARKET_ID_1.into()],
    )
    .unwrap();

    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "usdt".to_string(),
        "inj".to_string(),
        vec![TEST_MARKET_ID_2.into()],
    )
    .unwrap();

    let all_routes_result = get_all_swap_routes(deps.as_ref().storage);
    assert!(all_routes_result.is_ok(), "Error getting all routes");

    let eth_inj_route = SwapRoute {
        source_denom: "eth".to_string(),
        target_denom: "inj".to_string(),
        steps: vec![TEST_MARKET_ID_1.into(), TEST_MARKET_ID_2.into()],
    };

    let eth_usdt_route = SwapRoute {
        source_denom: "eth".to_string(),
        target_denom: "usdt".to_string(),
        steps: vec![TEST_MARKET_ID_1.into()],
    };

    let usdt_inj_route = SwapRoute {
        source_denom: "usdt".to_string(),
        target_denom: "inj".to_string(),
        steps: vec![TEST_MARKET_ID_2.into()],
    };

    let all_routes = all_routes_result.unwrap();
    assert_eq!(
        all_routes,
        vec![eth_inj_route, eth_usdt_route, usdt_inj_route],
        "Incorrect routes returned"
    );
}
