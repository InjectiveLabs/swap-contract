use cosmwasm_std::{coin, Addr};

use injective_test_tube::RunnerError::{ExecuteError, QueryError};
use injective_test_tube::{
    Account, Bank, Exchange, Gov, InjectiveTestApp, Module, RunnerError, RunnerResult,
    SigningAccount, Wasm,
};

use injective_math::{round_to_min_tick, FPDecimal};

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::testing::test_utils::{
    are_fpdecimals_approximately_equal, assert_fee_is_as_expected, create_limit_order,
    fund_account_with_some_inj, human_to_dec, init_contract_with_fee_recipient_and_get_address,
    init_default_signer_account, init_default_validator_account, init_rich_account,
    init_self_relaying_contract_and_get_address, launch_spot_market, must_init_account_with_funds,
    pause_spot_market, query_all_bank_balances, query_bank_balance, set_route_and_assert_success,
    str_coin, Decimals, OrderSide, ATOM, DEFAULT_ATOMIC_MULTIPLIER, DEFAULT_RELAYER_SHARE,
    DEFAULT_SELF_RELAYING_FEE_PART, DEFAULT_TAKER_FEE, ETH, INJ, USDC, USDT,
};
use crate::types::{FPCoin, SwapEstimationResult};

/*
   This suite of tests focuses on calculation logic itself and doesn't attempt to use neither
   realistic market configuration nor order prices, so that we don't have to deal with scaling issues.

   Hardcoded values used in these tests come from the first tab of this spreadsheet:
   https://docs.google.com/spreadsheets/d/1-0epjX580nDO_P2mm1tSjhvjJVppsvrO1BC4_wsBeyA/edit?usp=sharing
*/

#[test]
fn it_executes_a_swap_between_two_base_assets_with_multiple_price_levels() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: FPDecimal::from(12u128),
            },
        )
        .unwrap();

    assert_eq!(
        query_result.result_quantity,
        FPDecimal::must_from_str("2893.886"), //slightly rounded down
        "incorrect swap result estimate returned by query"
    );

    assert_eq!(
        query_result.expected_fees.len(),
        2,
        "Wrong number of fee denoms received"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: FPDecimal::must_from_str("3541.5"),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: FPDecimal::must_from_str("3530.891412"),
            denom: "usdt".to_string(),
        },
    ];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        FPDecimal::must_from_str("0.000001"),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        FPDecimal::must_from_str("2893"),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_balance_usdt_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());
    let contract_balance_usdt_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());

    assert!(
        contract_balance_usdt_after >= contract_balance_usdt_before,
        "Contract lost some money after swap. Balance before: {}, after: {}",
        contract_balance_usdt_before,
        contract_balance_usdt_after
    );

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_balance_usdt_after,
            contract_balance_usdt_before,
            max_diff,
        ),
        "Contract balance changed too much. Before: {}, After: {}",
        contract_balances_before[0].amount,
        contract_balances_after[0].amount
    );
}

#[test]
fn it_executes_a_swap_between_two_base_assets_with_single_price_level() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(3, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let expected_atom_estimate_quantity = FPDecimal::must_from_str("751.492");
    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: FPDecimal::from(3u128),
            },
        )
        .unwrap();

    assert_eq!(
        query_result.result_quantity, expected_atom_estimate_quantity,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: FPDecimal::must_from_str("904.5"),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: FPDecimal::must_from_str("901.790564"),
            denom: "usdt".to_string(),
        },
    ];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.00001", Decimals::Six),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(750u128),
        },
        &[coin(3, ETH)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        expected_atom_estimate_quantity.int(),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_executes_swap_between_markets_using_different_quote_assets() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDC);
    let spot_market_3_id = launch_spot_market(&exchange, &owner, USDC, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[
            str_coin("100_000", USDC, Decimals::Six),
            str_coin("100_000", USDT, Decimals::Six),
        ],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_3_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    //USDT-USDC
    create_limit_order(
        &app,
        &trader3,
        &spot_market_3_id,
        OrderSide::Sell,
        1,
        100_000_000,
    );

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: FPDecimal::from(12u128),
            },
        )
        .unwrap();

    // expected amount is a bit lower, even though 1 USDT = 1 USDC, because of the fees
    assert_eq!(
        query_result.result_quantity,
        FPDecimal::must_from_str("2889.64"),
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: FPDecimal::must_from_str("3541.5"),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: FPDecimal::must_from_str("3530.891412"),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: FPDecimal::must_from_str("3525.603007"),
            denom: "usdc".to_string(),
        },
    ];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.000001", Decimals::Six),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        FPDecimal::must_from_str("2889"),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        2,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_swap_between_markets_using_different_quote_asset_if_one_quote_buffer_is_insufficient()
{
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDC);
    let spot_market_3_id = launch_spot_market(&exchange, &owner, USDC, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[
            str_coin("0.0001", USDC, Decimals::Six),
            str_coin("100_000", USDT, Decimals::Six),
        ],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_3_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    //USDT-USDC
    create_limit_order(
        &app,
        &trader3,
        &spot_market_3_id,
        OrderSide::Sell,
        1,
        100_000_000,
    );

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(12u128),
        },
    );

    assert!(query_result.is_err(), "swap should have failed");
    assert!(
        query_result
            .unwrap_err()
            .to_string()
            .contains("Swap amount too high"),
        "incorrect query result error message"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert!(execute_result.is_err(), "swap should have failed");
    assert!(
        execute_result
            .unwrap_err()
            .to_string()
            .contains("Swap amount too high"),
        "incorrect query result error message"
    );

    let source_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let target_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        source_balance,
        FPDecimal::must_from_str("12"),
        "source balance should not have changed after failed swap"
    );
    assert_eq!(
        target_balance,
        FPDecimal::zero(),
        "target balance should not have changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        2,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_executes_a_sell_of_base_asset_to_receive_min_output_quantity() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        USDT,
        vec![spot_market_1_id.as_str().into()],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: USDT.to_string(),
                from_quantity: FPDecimal::from(12u128),
            },
        )
        .unwrap();

    // calculate how much can be USDT can be bought for 12 ETH without fees
    let orders_nominal_total_value = FPDecimal::from(201_000u128) * FPDecimal::from(5u128)
        + FPDecimal::from(195_000u128) * FPDecimal::from(4u128)
        + FPDecimal::from(192_000u128) * FPDecimal::from(3u128);
    let expected_target_quantity = orders_nominal_total_value
        * (FPDecimal::one()
            - FPDecimal::must_from_str(&format!(
                "{}",
                DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_SELF_RELAYING_FEE_PART
            )));

    assert_eq!(
        query_result.result_quantity, expected_target_quantity,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![FPCoin {
        amount: FPDecimal::must_from_str("3541.5"),
        denom: "usdt".to_string(),
    }];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        FPDecimal::must_from_str("0.000001"),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: USDT.to_string(),
            min_output_quantity: FPDecimal::from(2357458u128),
        },
        &[coin(12, ETH)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, USDT, swapper.address().as_str());
    let expected_execute_result = expected_target_quantity.int();

    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance, expected_execute_result,
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_executes_a_buy_of_base_asset_to_receive_min_output_quantity() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        USDT,
        vec![spot_market_1_id.as_str().into()],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);

    create_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Sell,
        201_000,
        5,
    );
    create_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Sell,
        195_000,
        4,
    );
    create_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Sell,
        192_000,
        3,
    );

    app.increase_time(1);

    let swapper_usdt = 2_360_995;
    let swapper = must_init_account_with_funds(
        &app,
        &[
            coin(swapper_usdt, USDT),
            str_coin("500_000", INJ, Decimals::Eighteen),
        ],
    );

    // calculate how much ETH we can buy with USDT we have
    let available_usdt_after_fee = FPDecimal::from(swapper_usdt)
        / (FPDecimal::one()
            + FPDecimal::must_from_str(&format!(
                "{}",
                DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_SELF_RELAYING_FEE_PART
            )));
    let usdt_left_for_most_expensive_order = available_usdt_after_fee
        - (FPDecimal::from(195_000u128) * FPDecimal::from(4u128)
            + FPDecimal::from(192_000u128) * FPDecimal::from(3u128));
    let most_expensive_order_quantity =
        usdt_left_for_most_expensive_order / FPDecimal::from(201_000u128);
    let expected_quantity =
        most_expensive_order_quantity + (FPDecimal::from(4u128) + FPDecimal::from(3u128));

    // round to min tick
    let expected_quantity_rounded =
        round_to_min_tick(expected_quantity, FPDecimal::must_from_str("0.001"));

    // calculate dust notional value as this will be the portion of user's funds that will stay in the contract
    let dust = expected_quantity - expected_quantity_rounded;
    // we need to use worst priced order
    let dust_value = dust * FPDecimal::from(201_000u128);

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: USDT.to_string(),
                target_denom: ETH.to_string(),
                from_quantity: FPDecimal::from(swapper_usdt),
            },
        )
        .unwrap();

    assert_eq!(
        query_result.result_quantity, expected_quantity_rounded,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![FPCoin {
        amount: FPDecimal::must_from_str("3536.188217"),
        denom: "usdt".to_string(),
    }];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        FPDecimal::must_from_str("0.000001"),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ETH.to_string(),
            min_output_quantity: FPDecimal::from(11u128),
        },
        &[coin(swapper_usdt, USDT)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, USDT, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let expected_execute_result = expected_quantity.int();

    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance, expected_execute_result,
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    let mut expected_contract_balances_after =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str()) + dust_value;
    expected_contract_balances_after = expected_contract_balances_after.int();

    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str()),
        expected_contract_balances_after,
        "contract balance changed unexpectedly after swap"
    );
}

#[test]
fn it_executes_a_swap_between_base_assets_with_external_fee_recipient() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let fee_recipient = must_init_account_with_funds(&app, &[]);
    let contr_addr = init_contract_with_fee_recipient_and_get_address(
        &wasm,
        &owner,
        &[str_coin("10_000", USDT, Decimals::Six)],
        &fee_recipient,
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    // calculate relayer's share of the fee based on assumptions that all orders are matched
    let buy_orders_nominal_total_value = FPDecimal::from(201_000u128) * FPDecimal::from(5u128)
        + FPDecimal::from(195_000u128) * FPDecimal::from(4u128)
        + FPDecimal::from(192_000u128) * FPDecimal::from(3u128);
    let relayer_sell_fee = buy_orders_nominal_total_value
        * FPDecimal::must_from_str(&format!(
            "{}",
            DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_RELAYER_SHARE
        ));

    // calculate relayer's share of the fee based on assumptions that some of orders are matched
    let expected_nominal_buy_most_expensive_match_quantity =
        FPDecimal::must_from_str("488.2222155454736648");
    let sell_orders_nominal_total_value = FPDecimal::from(800u128) * FPDecimal::from(800u128)
        + FPDecimal::from(810u128) * FPDecimal::from(800u128)
        + FPDecimal::from(820u128) * FPDecimal::from(800u128)
        + FPDecimal::from(830u128) * expected_nominal_buy_most_expensive_match_quantity;
    let relayer_buy_fee = sell_orders_nominal_total_value
        * FPDecimal::must_from_str(&format!(
            "{}",
            DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_RELAYER_SHARE
        ));
    let expected_fee_for_fee_recipient = relayer_buy_fee + relayer_sell_fee;

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: FPDecimal::from(12u128),
            },
        )
        .unwrap();

    assert_eq!(
        query_result.result_quantity,
        FPDecimal::must_from_str("2888.221"), //slightly rounded down vs spreadsheet
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: FPDecimal::must_from_str("5902.5"),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: FPDecimal::must_from_str("5873.061097"),
            denom: "usdt".to_string(),
        },
    ];

    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        FPDecimal::must_from_str("0.000001"),
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2888u128),
        },
        &[coin(12, ETH)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        FPDecimal::must_from_str("2888"),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_balance_usdt_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());
    let contract_balance_usdt_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());

    assert!(
        contract_balance_usdt_after >= contract_balance_usdt_before,
        "Contract lost some money after swap. Balance before: {}, after: {}",
        contract_balance_usdt_before,
        contract_balance_usdt_after
    );

    let max_diff = human_to_dec("0.00001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_balance_usdt_after,
            contract_balance_usdt_before,
            max_diff,
        ),
        "Contract balance changed too much. Before: {}, After: {}",
        contract_balances_before[0].amount,
        contract_balances_after[0].amount
    );

    let fee_recipient_balance = query_all_bank_balances(&bank, &fee_recipient.address());

    assert_eq!(
        fee_recipient_balance.len(),
        1,
        "wrong number of denoms in fee recipient's balances"
    );
    assert_eq!(
        fee_recipient_balance[0].denom, USDT,
        "fee recipient did not receive fee in expected denom"
    );
    assert_eq!(
        FPDecimal::must_from_str(fee_recipient_balance[0].amount.as_str()),
        expected_fee_for_fee_recipient.int(),
        "fee recipient did not receive expected fee"
    );
}

#[test]
fn it_reverts_the_swap_if_there_isnt_enough_buffer_for_buying_target_asset() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("0.001", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(12u128),
        },
    );

    assert!(query_result.is_err(), "query should fail");
    assert!(
        query_result
            .unwrap_err()
            .to_string()
            .contains("Swap amount too high"),
        "wrong query error message"
    );

    let contract_balances_before = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert!(execute_result.is_err(), "execute should fail");
    assert!(
        execute_result
            .unwrap_err()
            .to_string()
            .contains("Swap amount too high"),
        "wrong execute error message"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changes after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changes after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_swap_if_no_funds_were_passed() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let contract_balances_before = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[],
        &swapper,
    );
    let expected_error = RunnerError::ExecuteError { msg: "failed to execute message; message index: 0: Custom Error: \"Only one denom can be passed in funds\": execute wasm contract failed".to_string() };
    assert_eq!(
        execute_result.unwrap_err(),
        expected_error,
        "wrong error message"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changes after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changes after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());

    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_swap_if_multiple_funds_were_passed() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let eth_balance = 12u128;
    let atom_balance = 10u128;

    let swapper = must_init_account_with_funds(
        &app,
        &[
            coin(eth_balance, ETH),
            coin(atom_balance, ATOM),
            str_coin("500_000", INJ, Decimals::Eighteen),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(10u128),
        },
        &[coin(10, ATOM), coin(12, ETH)],
        &swapper,
    );
    assert!(
        execute_result
            .unwrap_err()
            .to_string()
            .contains("Only one denom can be passed in funds"),
        "wrong error message"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(eth_balance),
        "wrong ETH balance after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::from(atom_balance),
        "wrong ATOM balance after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_if_user_passes_quantities_equal_to_zero() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(0u128),
        },
    );
    assert!(
        query_result
            .unwrap_err()
            .to_string()
            .contains("source_quantity must be positive"),
        "incorrect error returned by query"
    );

    let contract_balances_before = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let err = wasm
        .execute(
            &contr_addr,
            &ExecuteMsg::SwapMinOutput {
                target_denom: ATOM.to_string(),
                min_output_quantity: FPDecimal::zero(),
            },
            &[coin(12, ETH)],
            &swapper,
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Output quantity must be positive!"),
        "incorrect error returned by execute"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::must_from_str("12"),
        "swap should not have occurred"
    );
    assert_eq!(
        to_balance,
        FPDecimal::must_from_str("0"),
        "swapper should not have received any target tokens"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_if_user_passes_negative_quantities() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    app.increase_time(1);

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::must_from_str("-1"),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert!(
        execute_result.is_err(),
        "swap with negative minimum amount to receive did not fail"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after failed swap"
    );
}

#[test]
fn it_reverts_if_there_arent_enough_orders_to_satisfy_min_quantity() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);

    create_limit_order(&app, &trader1, &spot_market_2_id, OrderSide::Sell, 800, 800);
    create_limit_order(&app, &trader2, &spot_market_2_id, OrderSide::Sell, 810, 800);
    create_limit_order(&app, &trader3, &spot_market_2_id, OrderSide::Sell, 820, 800);
    create_limit_order(&app, &trader1, &spot_market_2_id, OrderSide::Sell, 830, 450); //not enough for minimum requested

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(12u128),
        },
    );
    assert_eq!(
        query_result.unwrap_err(),
        QueryError {
            msg: "Generic error: Not enough liquidity to fulfill order: query wasm contract failed"
                .to_string()
        },
        "wrong error message"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert_eq!(execute_result.unwrap_err(), RunnerError::ExecuteError { msg: "failed to execute message; message index: 0: dispatch: submessages: reply: Generic error: Not enough liquidity to fulfill order: execute wasm contract failed".to_string() }, "wrong error message");

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after swap"
    );
}

#[test]
fn it_reverts_if_min_quantity_cannot_be_reached() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    // set the market
    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let min_quantity = 3500u128;
    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(min_quantity),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert_eq!(execute_result.unwrap_err(), RunnerError::ExecuteError { msg: format!("failed to execute message; message index: 0: dispatch: submessages: reply: dispatch: submessages: reply: Min expected swap amount ({min_quantity}) not reached: execute wasm contract failed") }, "wrong error message");

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after failed swap"
    );
}

#[test]
fn it_reverts_if_market_is_paused() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);
    let gov = Gov::new(&app);

    let signer = init_default_signer_account(&app);
    let validator = init_default_validator_account(&app);
    fund_account_with_some_inj(&bank, &signer, &validator);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    pause_spot_market(&gov, spot_market_1_id.as_str(), &signer, &validator);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let swapper = must_init_account_with_funds(
        &app,
        &[coin(12, ETH), str_coin("500_000", INJ, Decimals::Eighteen)],
    );

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(12u128),
        },
    );

    assert!(
        query_result
            .unwrap_err()
            .to_string()
            .contains("Querier contract error"),
        "wrong error returned by query"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert!(
        execute_result
            .unwrap_err()
            .to_string()
            .contains("Querier contract error"),
        "wrong error returned by execute"
    );

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after failed swap"
    );
}

#[test]
fn it_reverts_if_user_doesnt_have_enough_inj_to_pay_for_gas() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = init_default_signer_account(&app);
    let _validator = init_default_validator_account(&app);
    let owner = init_rich_account(&app);

    let spot_market_1_id = launch_spot_market(&exchange, &owner, ETH, USDT);
    let spot_market_2_id = launch_spot_market(&exchange, &owner, ATOM, USDT);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("100_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        ETH,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let swapper = must_init_account_with_funds(&app, &[coin(12, ETH), coin(10, INJ)]);

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_eth_buy_orders(&app, &spot_market_1_id, &trader1, &trader2);
    create_atom_sell_orders(&app, &spot_market_2_id, &trader1, &trader2, &trader3);

    app.increase_time(1);

    let query_result: RunnerResult<SwapEstimationResult> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: FPDecimal::from(12u128),
        },
    );

    let target_quantity = query_result.unwrap().result_quantity;

    assert_eq!(
        target_quantity,
        FPDecimal::must_from_str("2893.886"), //slightly underestimated vs spreadsheet
        "incorrect swap result estimate returned by query"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapMinOutput {
            target_denom: ATOM.to_string(),
            min_output_quantity: FPDecimal::from(2800u128),
        },
        &[coin(12, ETH)],
        &swapper,
    );

    assert_eq!(execute_result.unwrap_err(), ExecuteError { msg: "spendable balance 10inj is smaller than 2500inj: insufficient funds: insufficient funds".to_string() }, "wrong error returned by execute");

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::from(12u128),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::zero(),
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balance has changed after failed swap"
    );
}

#[test]
fn it_allows_admin_to_withdraw_all_funds_from_contract_to_his_address() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);

    let usdt_to_withdraw = str_coin("10_000", USDT, Decimals::Six);
    let eth_to_withdraw = str_coin("0.00062", ETH, Decimals::Eighteen);

    let owner = must_init_account_with_funds(
        &app,
        &[
            eth_to_withdraw.clone(),
            str_coin("1", INJ, Decimals::Eighteen),
            usdt_to_withdraw.clone(),
        ],
    );

    let initial_contract_balance = &[eth_to_withdraw, usdt_to_withdraw];
    let contr_addr =
        init_self_relaying_contract_and_get_address(&wasm, &owner, initial_contract_balance);

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::WithdrawSupportFunds {
            coins: initial_contract_balance.to_vec(),
            target_address: Addr::unchecked(owner.address()),
        },
        &[],
        &owner,
    );

    assert!(execute_result.is_ok(), "failed to withdraw support funds");
    let contract_balances_after = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_after.len(),
        0,
        "contract had some balances after withdraw"
    );

    let owner_eth_balance = query_bank_balance(&bank, ETH, owner.address().as_str());
    assert_eq!(
        owner_eth_balance,
        FPDecimal::from(initial_contract_balance[0].amount),
        "wrong owner eth balance after withdraw"
    );

    let owner_usdt_balance = query_bank_balance(&bank, USDT, owner.address().as_str());
    assert_eq!(
        owner_usdt_balance,
        FPDecimal::from(initial_contract_balance[1].amount),
        "wrong owner usdt balance after withdraw"
    );
}

#[test]
fn it_allows_admin_to_withdraw_all_funds_from_contract_to_other_address() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);

    let usdt_to_withdraw = str_coin("10_000", USDT, Decimals::Six);
    let eth_to_withdraw = str_coin("0.00062", ETH, Decimals::Eighteen);

    let owner = must_init_account_with_funds(
        &app,
        &[
            eth_to_withdraw.clone(),
            str_coin("1", INJ, Decimals::Eighteen),
            usdt_to_withdraw.clone(),
        ],
    );

    let initial_contract_balance = &[eth_to_withdraw, usdt_to_withdraw];
    let contr_addr =
        init_self_relaying_contract_and_get_address(&wasm, &owner, initial_contract_balance);

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    let random_dude = must_init_account_with_funds(&app, &[]);

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::WithdrawSupportFunds {
            coins: initial_contract_balance.to_vec(),
            target_address: Addr::unchecked(random_dude.address()),
        },
        &[],
        &owner,
    );

    assert!(execute_result.is_ok(), "failed to withdraw support funds");
    let contract_balances_after = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_after.len(),
        0,
        "contract had some balances after withdraw"
    );

    let random_dude_eth_balance = query_bank_balance(&bank, ETH, random_dude.address().as_str());
    assert_eq!(
        random_dude_eth_balance,
        FPDecimal::from(initial_contract_balance[0].amount),
        "wrong owner eth balance after withdraw"
    );

    let random_dude_usdt_balance = query_bank_balance(&bank, USDT, random_dude.address().as_str());
    assert_eq!(
        random_dude_usdt_balance,
        FPDecimal::from(initial_contract_balance[1].amount),
        "wrong owner usdt balance after withdraw"
    );
}

#[test]
fn it_doesnt_allow_non_admin_to_withdraw_anything_from_contract() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);

    let usdt_to_withdraw = str_coin("10_000", USDT, Decimals::Six);
    let eth_to_withdraw = str_coin("0.00062", ETH, Decimals::Eighteen);

    let owner = must_init_account_with_funds(
        &app,
        &[
            eth_to_withdraw.clone(),
            str_coin("1", INJ, Decimals::Eighteen),
            usdt_to_withdraw.clone(),
        ],
    );

    let initial_contract_balance = &[eth_to_withdraw, usdt_to_withdraw];
    let contr_addr =
        init_self_relaying_contract_and_get_address(&wasm, &owner, initial_contract_balance);

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    let random_dude = must_init_account_with_funds(&app, &[coin(1_000_000_000_000, INJ)]);

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::WithdrawSupportFunds {
            coins: initial_contract_balance.to_vec(),
            target_address: Addr::unchecked(owner.address()),
        },
        &[],
        &random_dude,
    );

    assert!(
        execute_result.is_err(),
        "succeeded to withdraw support funds"
    );
    let contract_balances_after = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_after, contract_balances_before,
        "contract balances changed after failed withdraw"
    );

    let random_dude_eth_balance = query_bank_balance(&bank, ETH, random_dude.address().as_str());
    assert_eq!(
        random_dude_eth_balance,
        FPDecimal::zero(),
        "random dude has some eth balance after failed withdraw"
    );

    let random_dude_usdt_balance = query_bank_balance(&bank, USDT, random_dude.address().as_str());
    assert_eq!(
        random_dude_usdt_balance,
        FPDecimal::zero(),
        "random dude has some usdt balance after failed withdraw"
    );
}

fn create_eth_buy_orders(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
) {
    create_limit_order(app, trader1, market_id, OrderSide::Buy, 201_000, 5);
    create_limit_order(app, trader2, market_id, OrderSide::Buy, 195_000, 4);
    create_limit_order(app, trader2, market_id, OrderSide::Buy, 192_000, 3);
}

fn create_atom_sell_orders(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
    trader3: &SigningAccount,
) {
    create_limit_order(app, trader1, market_id, OrderSide::Sell, 800, 800);
    create_limit_order(app, trader2, market_id, OrderSide::Sell, 810, 800);
    create_limit_order(app, trader3, market_id, OrderSide::Sell, 820, 800);
    create_limit_order(app, trader1, market_id, OrderSide::Sell, 830, 800);
}
