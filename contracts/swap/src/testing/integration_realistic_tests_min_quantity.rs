use injective_test_tube::{
    Account, Bank, Exchange, InjectiveTestApp, Module, RunnerResult, SigningAccount, Wasm,
};
use std::ops::Neg;

use crate::helpers::Scaled;
use injective_math::FPDecimal;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::testing::test_utils::{
    are_fpdecimals_approximately_equal, assert_fee_is_as_expected,
    create_realistic_atom_usdt_sell_orders_from_spreadsheet,
    create_realistic_eth_usdt_buy_orders_from_spreadsheet,
    create_realistic_eth_usdt_sell_orders_from_spreadsheet,
    create_realistic_inj_usdt_buy_orders_from_spreadsheet,
    create_realistic_usdt_usdc_both_side_orders, human_to_dec, init_rich_account,
    init_self_relaying_contract_and_get_address, launch_realistic_atom_usdt_spot_market,
    launch_realistic_inj_usdt_spot_market, launch_realistic_usdt_usdc_spot_market,
    launch_realistic_weth_usdt_spot_market, must_init_account_with_funds, query_all_bank_balances,
    query_bank_balance, set_route_and_assert_success, str_coin, Decimals, ATOM,
    DEFAULT_ATOMIC_MULTIPLIER, DEFAULT_SELF_RELAYING_FEE_PART, DEFAULT_TAKER_FEE, ETH, INJ, INJ_2,
    USDC, USDT,
};
use crate::types::{FPCoin, SwapEstimationResult};

/*
   This test suite focuses on using using realistic values both for spot markets and for orders and
   focuses on swaps requesting minimum amount.

   ATOM/USDT market parameters were taken from mainnet. ETH/USDT market parameters mirror WETH/USDT
   spot market on mainnet. INJ_2/USDT mirrors mainnet's INJ/USDT market (we used a different denom
   to avoid mixing balance changes related to swap with ones related to gas payments).

   Hardcoded values used in these tests come from the second tab of this spreadsheet:
   https://docs.google.com/spreadsheets/d/1-0epjX580nDO_P2mm1tSjhvjJVppsvrO1BC4_wsBeyA/edit?usp=sharing

   In all tests contract is configured to self-relay trades and thus receive a 60% fee discount.
*/

pub fn happy_path_two_hops_test(app: InjectiveTestApp, owner: SigningAccount, contr_addr: String) {
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

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

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let eth_to_swap = "4.08";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(eth_to_swap, ETH, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: human_to_dec(eth_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    // it's expected that it is slightly less than what's in the spreadsheet
    let expected_amount = human_to_dec("906.17", Decimals::Six);

    assert_eq!(
        query_result.result_quantity, expected_amount,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: human_to_dec("12.221313", Decimals::Six),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: human_to_dec("12.184704", Decimals::Six),
            denom: "usdt".to_string(),
        },
    ];

    // we don't care too much about decimal fraction of the fee
    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.1", Decimals::Six),
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
            min_output_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );

    assert!(
        to_balance >= expected_amount,
        "Swapper received less than expected minimum amount. Expected: {} ATOM, actual: {} ATOM",
        expected_amount.scaled(Decimals::Six.get_decimals().neg()),
        to_balance.scaled(Decimals::Six.get_decimals().neg()),
    );

    let max_diff = human_to_dec("0.1", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_amount,
            to_balance,
            max_diff,
        ),
        "Swapper did not receive expected amount. Expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        expected_amount.scaled(Decimals::Six.get_decimals().neg()),
        to_balance.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_usdt_balance_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());
    let contract_usdt_balance_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());

    assert!(
        contract_usdt_balance_after >= contract_usdt_balance_before,
        "Contract lost some money after swap. Actual balance: {} USDT, previous balance: {} USDT",
        contract_usdt_balance_after,
        contract_usdt_balance_before
    );

    // contract is allowed to earn extra 0.7 USDT from the swap of ~$8150 worth of ETH
    let max_diff = human_to_dec("0.7", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdt_balance_after,
            contract_usdt_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {} USDT, previous balance: {} USDT. Max diff: {} USDT",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

#[test]
fn happy_path_two_hops_swap_eth_atom_realistic_values_self_relaying() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("1_000", USDT, Decimals::Six)],
    );

    happy_path_two_hops_test(app, owner, contr_addr);
}

#[test]
fn happy_path_two_hops_swap_inj_eth_realistic_values_self_relaying() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
            str_coin("1", INJ_2, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_inj_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("1_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        INJ_2,
        ETH,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_realistic_inj_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_eth_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let inj_to_swap = "973.258";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(inj_to_swap, INJ_2, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: ETH.to_string(),
                from_quantity: human_to_dec(inj_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    // it's expected that it is slightly less than what's in the spreadsheet
    let expected_amount = human_to_dec("3.994", Decimals::Eighteen);

    assert_eq!(
        query_result.result_quantity, expected_amount,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: human_to_dec("12.73828775", Decimals::Six),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: human_to_dec("12.70013012", Decimals::Six),
            denom: "usdt".to_string(),
        },
    ];

    // we don't care too much about decimal fraction of the fee
    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.1", Decimals::Six),
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
            min_output_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );

    assert!(
        to_balance >= expected_amount,
        "Swapper received less than expected minimum amount. Expected: {} ETH, actual: {} ETH",
        expected_amount.scaled(Decimals::Eighteen.get_decimals().neg()),
        to_balance.scaled(Decimals::Eighteen.get_decimals().neg()),
    );

    let max_diff = human_to_dec("0.1", Decimals::Eighteen);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_amount,
            to_balance,
            max_diff,
        ),
        "Swapper did not receive expected amount. Expected: {} ETH, actual: {} ETH, max diff: {} ETH",
        expected_amount.scaled(Decimals::Eighteen.get_decimals().neg()),
        to_balance.scaled(Decimals::Eighteen.get_decimals().neg()),
        max_diff.scaled(Decimals::Eighteen.get_decimals().neg())
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_usdt_balance_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());
    let contract_usdt_balance_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());

    assert!(
        contract_usdt_balance_after >= contract_usdt_balance_before,
        "Contract lost some money after swap. Actual balance: {} USDT, previous balance: {} USDT",
        contract_usdt_balance_after,
        contract_usdt_balance_before
    );

    // contract is allowed to earn extra 0.7 USDT from the swap of ~$8150 worth of ETH
    let max_diff = human_to_dec("0.7", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdt_balance_after,
            contract_usdt_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {} USDT, previous balance: {} USDT. Max diff: {} USDT",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

#[test]
fn happy_path_two_hops_swap_inj_atom_realistic_values_self_relaying() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
            str_coin("1", INJ_2, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_inj_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("1_000", USDT, Decimals::Six)],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        INJ_2,
        ATOM,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_realistic_inj_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let inj_to_swap = "973.258";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(inj_to_swap, INJ_2, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: human_to_dec(inj_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    // it's expected that it is slightly less than what's in the spreadsheet
    let expected_amount = human_to_dec("944.26", Decimals::Six);

    assert_eq!(
        query_result.result_quantity, expected_amount,
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: human_to_dec("12.73828775", Decimals::Six),
            denom: "usdt".to_string(),
        },
        FPCoin {
            amount: human_to_dec("12.70013012", Decimals::Six),
            denom: "usdt".to_string(),
        },
    ];

    // we don't care too much about decimal fraction of the fee
    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.1", Decimals::Six),
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
            min_output_quantity: FPDecimal::from(944u128),
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );

    assert!(
        to_balance >= expected_amount,
        "Swapper received less than expected minimum amount. Expected: {} ATOM, actual: {} ATOM",
        expected_amount.scaled(Decimals::Six.get_decimals().neg()),
        to_balance.scaled(Decimals::Six.get_decimals().neg()),
    );

    let max_diff = human_to_dec("0.1", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_amount,
            to_balance,
            max_diff,
        ),
        "Swapper did not receive expected amount. Expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        expected_amount.scaled(Decimals::Six.get_decimals().neg()),
        to_balance.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_usdt_balance_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());
    let contract_usdt_balance_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());

    assert!(
        contract_usdt_balance_after >= contract_usdt_balance_before,
        "Contract lost some money after swap. Actual balance: {} USDT, previous balance: {} USDT",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg())
    );

    // contract is allowed to earn extra 0.82 USDT from the swap of ~$8500 worth of INJ
    let max_diff = human_to_dec("0.82", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdt_balance_after,
            contract_usdt_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {}, previous balance: {}. Max diff: {}",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

#[test]
fn it_executes_swap_between_markets_using_different_quote_assets_self_relaying() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);
    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();

    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("1_000", USDC, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
            str_coin("1", INJ_2, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_inj_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_usdt_usdc_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[
            str_coin("10", USDC, Decimals::Six),
            str_coin("500", USDT, Decimals::Six),
        ],
    );
    set_route_and_assert_success(
        &wasm,
        &owner,
        &contr_addr,
        INJ_2,
        USDC,
        vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);

    create_realistic_inj_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_usdt_usdc_both_side_orders(&app, &spot_market_2_id, &trader1);

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", INJ, Decimals::Eighteen),
            str_coin("1", INJ_2, Decimals::Eighteen),
        ],
    );

    let inj_to_swap = "1";

    let mut query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: USDC.to_string(),
                from_quantity: human_to_dec(inj_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    let expected_amount = human_to_dec("8.867", Decimals::Six);
    let max_diff = human_to_dec("0.001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(expected_amount, query_result.result_quantity, max_diff),
        "incorrect swap result estimate returned by query"
    );

    let mut expected_fees = vec![
        FPCoin {
            amount: human_to_dec("0.013365", Decimals::Six),
            denom: USDT.to_string(),
        },
        FPCoin {
            amount: human_to_dec("0.01332", Decimals::Six),
            denom: USDC.to_string(),
        },
    ];

    // we don't care too much about decimal fraction of the fee
    assert_fee_is_as_expected(
        &mut query_result.expected_fees,
        &mut expected_fees,
        human_to_dec("0.1", Decimals::Six),
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
            target_denom: USDC.to_string(),
            min_output_quantity: FPDecimal::from(8u128),
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, USDC, swapper.address().as_str());

    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );

    assert!(
        to_balance >= expected_amount,
        "Swapper received less than expected minimum amount. Expected: {} USDC, actual: {} USDC",
        expected_amount.scaled(Decimals::Eighteen.get_decimals().neg()),
        to_balance.scaled(Decimals::Eighteen.get_decimals().neg()),
    );

    let max_diff = human_to_dec("0.1", Decimals::Eighteen);

    assert!(
        are_fpdecimals_approximately_equal(
            expected_amount,
            to_balance,
            max_diff,
        ),
        "Swapper did not receive expected amount. Expected: {} USDC, actual: {} USDC, max diff: {} USDC",
        expected_amount.scaled(Decimals::Eighteen.get_decimals().neg()),
        to_balance.scaled(Decimals::Eighteen.get_decimals().neg()),
        max_diff.scaled(Decimals::Eighteen.get_decimals().neg())
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        2,
        "wrong number of denoms in contract balances"
    );

    // let's check contract's USDT balance
    let contract_usdt_balance_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());
    let contract_usdt_balance_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());

    assert!(
        contract_usdt_balance_after >= contract_usdt_balance_before,
        "Contract lost some money after swap. Actual balance: {} USDT, previous balance: {} USDT",
        contract_usdt_balance_after,
        contract_usdt_balance_before
    );

    // contract is allowed to earn extra 0.001 USDT from the swap of ~$8 worth of INJ
    let max_diff = human_to_dec("0.001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdt_balance_after,
            contract_usdt_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {} USDT, previous balance: {} USDT. Max diff: {} USDT",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );

    // let's check contract's USDC balance
    let contract_usdc_balance_before =
        FPDecimal::must_from_str(contract_balances_before[1].amount.as_str());
    let contract_usdc_balance_after =
        FPDecimal::must_from_str(contract_balances_after[1].amount.as_str());

    assert!(
        contract_usdc_balance_after >= contract_usdc_balance_before,
        "Contract lost some money after swap. Actual balance: {} USDC, previous balance: {} USDC",
        contract_usdc_balance_after,
        contract_usdc_balance_before
    );

    // contract is allowed to earn extra 0.001 USDC from the swap of ~$8 worth of INJ
    let max_diff = human_to_dec("0.001", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdc_balance_after,
            contract_usdc_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {} USDC, previous balance: {} USDC. Max diff: {} USDC",
        contract_usdc_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdc_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

#[test]
fn it_doesnt_lose_buffer_if_executed_multiple_times() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();

    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("1_000", USDT, Decimals::Six)],
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

    let eth_to_swap = "4.08";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(
                (FPDecimal::must_from_str(eth_to_swap) * FPDecimal::from(100u128))
                    .to_string()
                    .as_str(),
                ETH,
                Decimals::Eighteen,
            ),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let mut counter = 0;
    let iterations = 100;

    while counter < iterations {
        create_realistic_eth_usdt_buy_orders_from_spreadsheet(
            &app,
            &spot_market_1_id,
            &trader1,
            &trader2,
        );
        create_realistic_atom_usdt_sell_orders_from_spreadsheet(
            &app,
            &spot_market_2_id,
            &trader1,
            &trader2,
            &trader3,
        );

        app.increase_time(1);

        wasm.execute(
            &contr_addr,
            &ExecuteMsg::SwapMinOutput {
                target_denom: ATOM.to_string(),
                min_output_quantity: FPDecimal::from(906u128),
            },
            &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
            &swapper,
        )
        .unwrap();

        counter += 1
    }

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
        "Contract lost some money after swap. Starting balance: {}, Current balance: {}",
        contract_balance_usdt_after,
        contract_balance_usdt_before
    );

    // single swap with the same values results in < 0.7 USDT earning, so we expected that 100 same swaps
    // won't change balance by more than 0.7 * 100 = 70 USDT
    let max_diff = human_to_dec("0.7", Decimals::Six) * FPDecimal::from(iterations as u128);

    assert!(are_fpdecimals_approximately_equal(
        contract_balance_usdt_after,
        contract_balance_usdt_before,
        max_diff,
    ),  "Contract balance changed too much. Starting balance: {}, Current balance: {}. Max diff: {}",
            contract_balance_usdt_before.scaled(Decimals::Six.get_decimals().neg()),
            contract_balance_usdt_after.scaled(Decimals::Six.get_decimals().neg()),
            max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

/*
   This test shows that query overestimates the amount of USDT needed to execute the swap. It seems
   that in reality we get a better price when selling ETH than the one returned by query and can
   execute the swap with less USDT.

   It's easiest to check by commenting out the query_result assert and running the test. It will
   pass and amounts will perfectly match our assertions.
*/
#[ignore]
#[test]
fn it_correctly_calculates_required_funds_when_querying_buy_with_minimum_buffer_and_realistic_values(
) {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("51", USDT, Decimals::Six)],
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

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let eth_to_swap = "4.08";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(eth_to_swap, ETH, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let query_result: FPDecimal = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetOutputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                from_quantity: human_to_dec(eth_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("906.195", Decimals::Six),
        "incorrect swap result estimate returned by query"
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
            min_output_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        human_to_dec("906.195", Decimals::Six),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let atom_amount_below_min_tick_size = FPDecimal::must_from_str("0.0005463");
    let mut dust_value = atom_amount_below_min_tick_size * human_to_dec("8.89", Decimals::Six);

    let fee_refund = dust_value
        * FPDecimal::must_from_str(&format!(
            "{}",
            DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_SELF_RELAYING_FEE_PART
        ));

    dust_value += fee_refund;

    let expected_contract_usdt_balance =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str()) + dust_value;
    let actual_contract_balance =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());
    let contract_balance_diff = expected_contract_usdt_balance - actual_contract_balance;

    // here the actual difference is 0.000067 USDT, which we attribute differences between decimal precision of Rust/Go and Google Sheets
    assert!(
        human_to_dec("0.0001", Decimals::Six) - contract_balance_diff > FPDecimal::ZERO,
        "contract balance has changed too much after swap"
    );
}

/*
   This test shows that in some edge cases we calculate required funds differently than the chain does.
   When estimating balance hold for atomic market order chain doesn't take into account whether sender is
   also fee recipient, while we do. This leads to a situation where we estimate required funds to be
   lower than what's expected by the chain, which makes the swap fail.

   In this test we skip query estimation and go straight to executing swap.
*/
#[ignore]
#[test]
fn it_correctly_calculates_required_funds_when_executing_buy_with_minimum_buffer_and_realistic_values(
) {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    // in reality we need to add at least 49 USDT to the buffer, even if according to contract's calculations 42 USDT would be enough to execute the swap
    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("42", USDT, Decimals::Six)],
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

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let eth_to_swap = "4.08";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(eth_to_swap, ETH, Decimals::Eighteen),
            str_coin("0.01", INJ, Decimals::Eighteen),
        ],
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
            min_output_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::ZERO,
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        human_to_dec("906.195", Decimals::Six),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let contract_usdt_balance_before =
        FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());
    let contract_usdt_balance_after =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());

    assert!(
        contract_usdt_balance_after >= contract_usdt_balance_before,
        "Contract lost some money after swap. Actual balance: {}, previous balance: {}",
        contract_usdt_balance_after,
        contract_usdt_balance_before
    );

    // contract can earn max of 0.7 USDT, when exchanging ETH worth ~$8150
    let max_diff = human_to_dec("0.7", Decimals::Six);

    assert!(
        are_fpdecimals_approximately_equal(
            contract_usdt_balance_after,
            contract_usdt_balance_before,
            max_diff,
        ),
        "Contract balance changed too much. Actual balance: {}, previous balance: {}. Max diff: {}",
        contract_usdt_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        contract_usdt_balance_before.scaled(Decimals::Six.get_decimals().neg()),
        max_diff.scaled(Decimals::Six.get_decimals().neg())
    );
}

#[test]
fn it_returns_all_funds_if_there_is_not_enough_buffer_realistic_values() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    // 41 USDT is just below the amount required to buy required ATOM amount
    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("41", USDT, Decimals::Six)],
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

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let eth_to_swap = "4.08";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(eth_to_swap, ETH, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let query_result: RunnerResult<FPDecimal> = wasm.query(
        &contr_addr,
        &QueryMsg::GetOutputQuantity {
            source_denom: ETH.to_string(),
            target_denom: ATOM.to_string(),
            from_quantity: human_to_dec(eth_to_swap, Decimals::Eighteen),
        },
    );

    assert!(query_result.is_err(), "query should fail");

    assert!(
        query_result
            .unwrap_err()
            .to_string()
            .contains("Swap amount too high"),
        "incorrect error message in query result"
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
            min_output_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    );

    assert!(execute_result.is_err(), "execute should fail");

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        from_balance,
        human_to_dec(eth_to_swap, Decimals::Eighteen),
        "source balance changed after failed swap"
    );
    assert_eq!(
        to_balance,
        FPDecimal::ZERO,
        "target balance changed after failed swap"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    assert_eq!(
        contract_balances_before[0].amount, contract_balances_after[0].amount,
        "contract balance has changed after failed swap"
    );
}
