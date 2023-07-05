use std::ops::Neg;
use std::str::FromStr;

use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{coin, Addr};

use crate::admin::set_route;
use crate::contract::instantiate;
use crate::helpers::Scaled;
use injective_cosmwasm::{OwnedDepsExt, TEST_MARKET_ID_1, TEST_MARKET_ID_2};
use injective_math::FPDecimal;

use crate::msg::{FeeRecipient, InstantiateMsg};
use crate::queries::{estimate_swap_result, SwapQuantity};
use crate::state::get_all_swap_routes;
use crate::testing::test_utils::{
    are_fpdecimals_approximately_equal, human_to_dec, mock_deps_eth_inj,
    mock_realistic_deps_eth_atom, Decimals, MultiplierQueryBehavior, TEST_USER_ADDR,
};
use crate::types::{FPCoin, SwapRoute};

/*
    Tests focusing on queries with all values were taken from this spreadsheet:
`   https://docs.google.com/spreadsheets/d/1-0epjX580nDO_P2mm1tSjhvjJVppsvrO1BC4_wsBeyA/edit?usp=sharing
*/

#[test]
fn test_calculate_swap_price_external_fee_recipient_from_source_quantity() {
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
        &mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantity::InputQuantity(FPDecimal::from_str("12").unwrap()),
    )
    .unwrap();

    // in spreadsheet we expect 2888.224, but contract rounds average price up to be sure it doesn't deplete buffer
    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("2888.221"),
        "Wrong amount of swap execution estimate received when using source quantity"
    );

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee entries received when using source quantity"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("5902.5"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("5873.061097"),
        denom: "usdt".to_string(),
    };

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_1.amount,
        actual_swap_result.expected_fees[0].amount,
        max_diff,
    ),  "Wrong amount of first trx fee received when using source quantity. Expected: {}, Actual: {}",
        expected_fee_1.amount,
        actual_swap_result.expected_fees[0].amount
    );

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_2.amount,
        actual_swap_result.expected_fees[1].amount,
        max_diff,
    ),  "Wrong amount of second trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_2.amount,
            actual_swap_result.expected_fees[1].amount
    );
}

#[test]
fn test_calculate_swap_price_external_fee_recipient_from_target_quantity() {
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
        &mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantity::OutputQuantity(FPDecimal::from_str("2888.221").unwrap()),
    )
    .unwrap();

    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("12"),
        "Wrong amount of swap execution estimate received when using target quantity"
    ); // value rounded to min tick

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee entries received when using target quantity"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("5873.061097"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("5902.5"),
        denom: "usdt".to_string(),
    };

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_1.amount,
        actual_swap_result.expected_fees[0].amount,
        max_diff,
    ),  "Wrong amount of first trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_1.amount,
            actual_swap_result.expected_fees[0].amount
    );

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_2.amount,
        actual_swap_result.expected_fees[1].amount,
        max_diff,
    ),  "Wrong amount of second trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_2.amount,
            actual_swap_result.expected_fees[1].amount
    );
}

#[test]
fn test_calculate_swap_price_self_fee_recipient_from_source_quantity() {
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
        &mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantity::InputQuantity(FPDecimal::from_str("12").unwrap()),
    )
    .unwrap();

    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("2893.886"),
        "Wrong amount of swap execution estimate received"
    ); // value rounded to min tick

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee entries received"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("3541.5"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("3530.891412"),
        denom: "usdt".to_string(),
    };

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_1.amount,
        actual_swap_result.expected_fees[0].amount,
        max_diff,
    ),  "Wrong amount of first trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_1.amount,
            actual_swap_result.expected_fees[0].amount
    );

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_2.amount,
        actual_swap_result.expected_fees[1].amount,
        max_diff,
    ),  "Wrong amount of second trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_2.amount,
            actual_swap_result.expected_fees[1].amount
    );
}

#[test]
fn test_calculate_swap_price_self_fee_recipient_from_target_quantity() {
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
        &mock_env(),
        "eth".to_string(),
        "inj".to_string(),
        SwapQuantity::OutputQuantity(FPDecimal::from_str("2893.886").unwrap()),
    )
    .unwrap();

    assert_eq!(
        actual_swap_result.result_quantity,
        FPDecimal::must_from_str("12"),
        "Wrong amount of swap execution estimate received when using target quantity"
    ); // value rounded to min tick

    assert_eq!(
        actual_swap_result.expected_fees.len(),
        2,
        "Wrong number of fee entries received when using target quantity"
    );

    // values from the spreadsheet
    let expected_fee_1 = FPCoin {
        amount: FPDecimal::must_from_str("3530.891412"),
        denom: "usdt".to_string(),
    };

    // values from the spreadsheet
    let expected_fee_2 = FPCoin {
        amount: FPDecimal::must_from_str("3541.5"),
        denom: "usdt".to_string(),
    };

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_1.amount,
        actual_swap_result.expected_fees[0].amount,
        max_diff,
    ),  "Wrong amount of first trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_1.amount,
            actual_swap_result.expected_fees[0].amount
    );

    assert!(are_fpdecimals_approximately_equal(
        expected_fee_2.amount,
        actual_swap_result.expected_fees[1].amount,
        max_diff,
    ),  "Wrong amount of second trx fee received when using source quantity. Expected: {}, Actual: {}",
            expected_fee_2.amount,
            actual_swap_result.expected_fees[1].amount
    );
}

// these values were not taken from spreadsheet, we just assume that both direction of estimate
// should be almost symmetrical (almost due to complex sequence of roundings)
#[test]
fn test_calculate_estimate_when_selling_both_quantity_directions_simple() {
    let mut deps = mock_realistic_deps_eth_atom(MultiplierQueryBehavior::Success);
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
        "usdt".to_string(),
        vec![TEST_MARKET_ID_1.into()],
    )
    .unwrap();

    let eth_input_amount = human_to_dec("4.08", Decimals::Eighteen);

    let input_swap_estimate = estimate_swap_result(
        deps.as_ref(),
        &mock_env(),
        "eth".to_string(),
        "usdt".to_string(),
        SwapQuantity::InputQuantity(eth_input_amount),
    )
    .unwrap();

    let expected_usdt_result_quantity = human_to_dec("8127.7324632", Decimals::Six);

    assert_eq!(
        input_swap_estimate.result_quantity, expected_usdt_result_quantity,
        "Wrong amount of swap execution estimate received when using source quantity"
    );

    assert_eq!(
        input_swap_estimate.expected_fees.len(),
        1,
        "Wrong number of fee entries received when using source quantity"
    );

    let expected_usdt_fee_amount = human_to_dec("20.3688555", Decimals::Six);

    let expected_fee = FPCoin {
        amount: expected_usdt_fee_amount,
        denom: "usdt".to_string(),
    };

    let max_diff = human_to_dec("0.1", Decimals::Eighteen);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_fee.amount,
            input_swap_estimate.expected_fees[0].amount,
            max_diff,
        ),
        "Wrong amount of trx fee received when using source quantity. Expected: {}, Actual: {}",
        expected_fee.amount,
        input_swap_estimate.expected_fees[0].amount
    );

    let output_swap_estimate = estimate_swap_result(
        deps.as_ref(),
        &mock_env(),
        "eth".to_string(),
        "usdt".to_string(),
        SwapQuantity::OutputQuantity(expected_usdt_result_quantity),
    )
    .unwrap();

    assert!(
        output_swap_estimate.result_quantity >= eth_input_amount,
        "Swap execution estimate when using target quantity wasn't higher than when using source quantity. Target amount: {} ETH, source amount: {} ETH",
        output_swap_estimate.result_quantity.scaled(Decimals::Eighteen.get_decimals().neg()),
        eth_input_amount.scaled(Decimals::Eighteen.get_decimals().neg())
    );

    assert!(
        are_fpdecimals_approximately_equal(
            output_swap_estimate.result_quantity,
            eth_input_amount,
            max_diff
        ),
        "Wrong amount of swap execution estimate received when using target quantity"
    );

    assert_eq!(
        output_swap_estimate.expected_fees.len(),
        1,
        "Wrong number of fee entries received when using target quantity"
    );

    let max_diff = human_to_dec("0.1", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_fee.amount,
            input_swap_estimate.expected_fees[0].amount,
            max_diff,
        ),
        "Wrong amount of trx fee received when using source quantity. Expected: {}, Actual: {}",
        expected_fee.amount,
        input_swap_estimate.expected_fees[0].amount
    );
}

// these values were not taken from spreadsheet, we just assume that both direction of estimate
// should be almost symmetrical (almost due to complex sequence of roundings); for some reason
// target estimate is slightly higher than source estimate (that should not be the case)
#[test]
fn test_calculate_estimate_when_buying_both_quantity_directions_simple() {
    let mut deps = mock_realistic_deps_eth_atom(MultiplierQueryBehavior::Success);
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
        "usdt".to_string(),
        vec![TEST_MARKET_ID_1.into()],
    )
    .unwrap();

    let usdt_input_amount = human_to_dec("8000", Decimals::Six);

    let input_swap_estimate = estimate_swap_result(
        deps.as_ref(),
        &mock_env(),
        "usdt".to_string(),
        "eth".to_string(),
        SwapQuantity::InputQuantity(usdt_input_amount),
    )
    .unwrap();

    let expected_eth_result_quantity = human_to_dec("3.994", Decimals::Eighteen);

    assert_eq!(
        input_swap_estimate.result_quantity, expected_eth_result_quantity,
        "Wrong amount of swap execution estimate received when using source quantity"
    );

    assert_eq!(
        input_swap_estimate.expected_fees.len(),
        1,
        "Wrong number of fee entries received when using source quantity"
    );

    let expected_usdt_fee_amount = human_to_dec("19.950124", Decimals::Six);

    let expected_fee = FPCoin {
        amount: expected_usdt_fee_amount,
        denom: "usdt".to_string(),
    };

    let mut max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_fee.amount,
            input_swap_estimate.expected_fees[0].amount,
            max_diff,
        ),
        "Wrong amount of trx fee received when using source quantity. Expected: {}, Actual: {}",
        expected_fee.amount,
        input_swap_estimate.expected_fees[0].amount
    );

    let output_swap_estimate = estimate_swap_result(
        deps.as_ref(),
        &mock_env(),
        "usdt".to_string(),
        "eth".to_string(),
        SwapQuantity::OutputQuantity(expected_eth_result_quantity),
    )
    .unwrap();

    // diff cannot be higher than 0.0025% of input amount
    max_diff = usdt_input_amount * FPDecimal::must_from_str("0.00025");

    assert!(
        are_fpdecimals_approximately_equal(
            output_swap_estimate.result_quantity,
            usdt_input_amount,
            max_diff
        ),
        "Wrong amount of swap execution estimate received when using target quantity"
    );

    assert!(
        are_fpdecimals_approximately_equal(
            expected_fee.amount,
            input_swap_estimate.expected_fees[0].amount,
            max_diff,
        ),
        "Wrong amount of trx fee received when using source quantity. Expected: {}, Actual: {}",
        expected_fee.amount,
        input_swap_estimate.expected_fees[0].amount
    );
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
