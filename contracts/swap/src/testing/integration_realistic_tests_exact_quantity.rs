use injective_test_tube::{Account, Bank, Exchange, InjectiveTestApp, Module, RunnerResult, Wasm};
use std::ops::Neg;

use crate::helpers::Scaled;
use injective_math::FPDecimal;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::testing::test_utils::{
    are_fpdecimals_approximately_equal, assert_fee_is_as_expected,
    create_realistic_atom_usdt_sell_orders_from_spreadsheet,
    create_realistic_eth_usdt_buy_orders_from_spreadsheet,
    create_realistic_inj_usdt_buy_orders_from_spreadsheet, create_realistic_limit_order,
    dec_to_proto, human_to_dec, init_default_validator_account, init_rich_account,
    init_self_relaying_contract_and_get_address, launch_custom_spot_market,
    launch_realistic_atom_usdt_spot_market, launch_realistic_inj_usdt_spot_market,
    launch_realistic_weth_usdt_spot_market, must_init_account_with_funds, query_all_bank_balances,
    query_bank_balance, set_route_and_assert_success, str_coin, Decimals, OrderSide, ATOM,
    DEFAULT_ATOMIC_MULTIPLIER, DEFAULT_SELF_RELAYING_FEE_PART, DEFAULT_TAKER_FEE, ETH, INJ, INJ_2,
    USDT,
};
use crate::types::{FPCoin, SwapEstimationResult};

/*
   This test suite focuses on using using realistic values both for spot markets and for orders.
   ATOM/USDT market parameters was taken from mainnet. ETH/USDT market parameters in reality
   mirror INJ/USDT spot market on mainnet (we did not want to use INJ/USDT market so that we don't
   mix balances changes coming from swap with those related to gas payment for contract execution).

   Hardcoded values used in these tests come from the second tab of this spreadsheet:
   https://docs.google.com/spreadsheets/d/1-0epjX580nDO_P2mm1tSjhvjJVppsvrO1BC4_wsBeyA/edit?usp=sharing
*/

struct Percent<'a>(&'a str);

// actual difference is ~2200%
#[test]
fn it_swaps_eth_to_get_minimum_exact_amount_of_atom_by_mightly_rounding_up() {
    exact_two_hop_eth_atom_swap_test_template(human_to_dec("0.01", Decimals::Six), Percent("2200"))
}

// actual difference is ~110%
#[test]
fn it_swaps_eth_to_get_very_low_exact_amount_of_atom_by_heavily_rounding_up() {
    exact_two_hop_eth_atom_swap_test_template(human_to_dec("0.11", Decimals::Six), Percent("110"))
}

//ok
#[test]
fn it_swaps_eth_to_get_low_exact_amount_of_atom_by_rounding_up() {
    exact_two_hop_eth_atom_swap_test_template(human_to_dec("4.12", Decimals::Six), Percent("10"))
}

//ok
#[test]
fn it_correctly_swaps_eth_to_get_normal_exact_amount_of_atom() {
    exact_two_hop_eth_atom_swap_test_template(human_to_dec("12.05", Decimals::Six), Percent("1"))
}

//ok
#[test]
fn it_correctly_swaps_eth_to_get_high_exact_amount_of_atom() {
    exact_two_hop_eth_atom_swap_test_template(human_to_dec("612", Decimals::Six), Percent("1"))
}

//ok
#[test]
fn it_correctly_swaps_eth_to_get_very_high_exact_amount_of_atom() {
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

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "2137.2",
        "2.78",
        Decimals::Eighteen,
        Decimals::Six,
    ); //order not present in the spreadsheet

    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.11",
        "321.11",
        Decimals::Six,
        Decimals::Six,
    ); //order not present in the spreadsheet

    app.increase_time(1);

    let eth_to_swap = "4.4";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(eth_to_swap, ETH, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let exact_quantity_to_receive = human_to_dec("1014.19", Decimals::Six);

    let query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetInputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                to_quantity: exact_quantity_to_receive,
            },
        )
        .unwrap();

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapExactOutput {
            target_denom: ATOM.to_string(),
            target_output_quantity: exact_quantity_to_receive,
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let expected_difference =
        human_to_dec(eth_to_swap, Decimals::Eighteen) - query_result.result_quantity;
    let swapper_eth_balance_after = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let swapper_atom_balance_after = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        swapper_eth_balance_after, expected_difference,
        "wrong amount of ETH was exchanged"
    );

    assert!(
        swapper_atom_balance_after >= exact_quantity_to_receive,
        "swapper got less than exact amount required -> expected: {} ATOM, actual: {} ATOM",
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg())
    );

    let one_percent_diff = exact_quantity_to_receive * FPDecimal::must_from_str("0.01");

    assert!(
        are_fpdecimals_approximately_equal(
            swapper_atom_balance_after,
            exact_quantity_to_receive,
            one_percent_diff,
        ),
        "swapper did not receive expected exact amount +/- 1% -> expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        one_percent_diff.scaled(Decimals::Six.get_decimals().neg())
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

    // contract is allowed to earn extra 0.73 USDT from the swap of ~$8450 worth of ETH
    let max_diff = human_to_dec("0.8", Decimals::Six);

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

// here we get exact amount
#[test]
fn it_swaps_inj_to_get_minimum_exact_amount_of_atom_by_mightly_rounding_up() {
    exact_two_hop_inj_atom_swap_test_template(human_to_dec("0.01", Decimals::Six), Percent("0"))
}

// here we get exact amount
#[test]
fn it_swaps_inj_to_get_very_low_exact_amount_of_atom() {
    exact_two_hop_inj_atom_swap_test_template(human_to_dec("0.11", Decimals::Six), Percent("0"))
}

// here we get exact amount
#[test]
fn it_swaps_inj_to_get_low_exact_amount_of_atom() {
    exact_two_hop_inj_atom_swap_test_template(human_to_dec("4.12", Decimals::Six), Percent("0"))
}

// here we get exact amount
#[test]
fn it_correctly_swaps_inj_to_get_normal_exact_amount_of_atom() {
    exact_two_hop_inj_atom_swap_test_template(human_to_dec("12.05", Decimals::Six), Percent("0"))
}

//ok
#[test]
fn it_correctly_swaps_inj_to_get_high_exact_amount_of_atom() {
    exact_two_hop_inj_atom_swap_test_template(human_to_dec("612", Decimals::Six), Percent("0.01"))
}

#[test]
fn it_correctly_swaps_inj_to_get_very_high_exact_amount_of_atom() {
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
            str_coin("10_000", INJ_2, Decimals::Eighteen),
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
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "8.99",
        "280.2",
        Decimals::Eighteen,
        Decimals::Six,
    ); //order not present in the spreadsheet

    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.11",
        "321.11",
        Decimals::Six,
        Decimals::Six,
    ); //order not present in the spreadsheet

    app.increase_time(1);

    let inj_to_swap = "1100.1";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(inj_to_swap, INJ_2, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let exact_quantity_to_receive = human_to_dec("1010.12", Decimals::Six);
    let max_diff_percentage = Percent("0.01");

    let query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetInputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: ATOM.to_string(),
                to_quantity: exact_quantity_to_receive,
            },
        )
        .unwrap();

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapExactOutput {
            target_denom: ATOM.to_string(),
            target_output_quantity: exact_quantity_to_receive,
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let expected_difference =
        human_to_dec(inj_to_swap, Decimals::Eighteen) - query_result.result_quantity;
    let swapper_inj_balance_after = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let swapper_atom_balance_after = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        swapper_inj_balance_after, expected_difference,
        "wrong amount of INJ was exchanged"
    );

    assert!(
        swapper_atom_balance_after >= exact_quantity_to_receive,
        "swapper got less than exact amount required -> expected: {} ATOM, actual: {} ATOM",
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg())
    );

    let one_percent_diff = exact_quantity_to_receive
        * (FPDecimal::must_from_str(max_diff_percentage.0) / FPDecimal::from(100u128));

    assert!(
        are_fpdecimals_approximately_equal(
            swapper_atom_balance_after,
            exact_quantity_to_receive,
            one_percent_diff,
        ),
        "swapper did not receive expected exact ATOM amount +/- {}% -> expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        max_diff_percentage.0,
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        one_percent_diff.scaled(Decimals::Six.get_decimals().neg())
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

    // contract is allowed to earn extra 0.7 USDT from the swap of ~$8150 worth of INJ
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

//ok
#[test]
fn it_doesnt_lose_buffer_if_exact_swap_of_eth_to_atom_is_executed_multiple_times() {
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
    let iterations = 100i128;

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(
                (FPDecimal::must_from_str(eth_to_swap) * FPDecimal::from(iterations))
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
            &ExecuteMsg::SwapExactOutput {
                target_denom: ATOM.to_string(),
                target_output_quantity: human_to_dec("906", Decimals::Six),
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
    let max_diff = human_to_dec("0.7", Decimals::Six) * FPDecimal::from(iterations);

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

#[test]
fn it_reverts_when_funds_provided_are_below_required_to_get_exact_amount() {
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
            str_coin("10_000", INJ_2, Decimals::Eighteen),
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

    let inj_to_swap = "608";

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin(inj_to_swap, INJ_2, Decimals::Eighteen),
            str_coin("1", INJ, Decimals::Eighteen),
        ],
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let exact_quantity_to_receive = human_to_dec("600", Decimals::Six);
    let swapper_inj_balance_before = query_bank_balance(&bank, INJ_2, swapper.address().as_str());

    let _: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetInputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: ATOM.to_string(),
                to_quantity: exact_quantity_to_receive,
            },
        )
        .unwrap();

    let execute_result = wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapExactOutput {
            target_denom: ATOM.to_string(),
            target_output_quantity: exact_quantity_to_receive,
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap_err();

    assert!(execute_result.to_string().contains("Provided amount of 608000000000000000000 is below required amount of 609714000000000000000"), "wrong error message");

    let swapper_inj_balance_after = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let swapper_atom_balance_after = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        swapper_inj_balance_before, swapper_inj_balance_after,
        "some amount of INJ was exchanged"
    );

    assert_eq!(
        FPDecimal::zero(), swapper_atom_balance_after,
        "swapper received some ATOM"
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

    assert_eq!(
        contract_usdt_balance_after, contract_usdt_balance_before,
        "Contract's balance changed after failed swap",
    );
}

// TEST TEMPLATES

fn exact_two_hop_eth_atom_swap_test_template(
    exact_quantity_to_receive: FPDecimal,
    max_diff_percentage: Percent,
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

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetInputQuantity {
                source_denom: ETH.to_string(),
                target_denom: ATOM.to_string(),
                to_quantity: exact_quantity_to_receive,
            },
        )
        .unwrap();

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapExactOutput {
            target_denom: ATOM.to_string(),
            target_output_quantity: exact_quantity_to_receive,
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let expected_difference =
        human_to_dec(eth_to_swap, Decimals::Eighteen) - query_result.result_quantity;
    let swapper_eth_balance_after = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let swapper_atom_balance_after = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        swapper_eth_balance_after, expected_difference,
        "wrong amount of ETH was exchanged"
    );

    let one_percent_diff = exact_quantity_to_receive
        * (FPDecimal::must_from_str(max_diff_percentage.0) / FPDecimal::from(100u128));

    assert!(
        swapper_atom_balance_after >= exact_quantity_to_receive,
        "swapper got less than exact amount required -> expected: {} ATOM, actual: {} ATOM",
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg())
    );

    assert!(
        are_fpdecimals_approximately_equal(
            swapper_atom_balance_after,
            exact_quantity_to_receive,
            one_percent_diff,
        ),
        "swapper did not receive expected exact amount +/- {}% -> expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        max_diff_percentage.0,
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        one_percent_diff.scaled(Decimals::Six.get_decimals().neg())
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

fn exact_two_hop_inj_atom_swap_test_template(
    exact_quantity_to_receive: FPDecimal,
    max_diff_percentage: Percent,
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
            str_coin("10_000", INJ_2, Decimals::Eighteen),
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

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let query_result: SwapEstimationResult = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetInputQuantity {
                source_denom: INJ_2.to_string(),
                target_denom: ATOM.to_string(),
                to_quantity: exact_quantity_to_receive,
            },
        )
        .unwrap();

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::SwapExactOutput {
            target_denom: ATOM.to_string(),
            target_output_quantity: exact_quantity_to_receive,
        },
        &[str_coin(inj_to_swap, INJ_2, Decimals::Eighteen)],
        &swapper,
    )
    .unwrap();

    let expected_difference =
        human_to_dec(inj_to_swap, Decimals::Eighteen) - query_result.result_quantity;
    let swapper_inj_balance_after = query_bank_balance(&bank, INJ_2, swapper.address().as_str());
    let swapper_atom_balance_after = query_bank_balance(&bank, ATOM, swapper.address().as_str());

    assert_eq!(
        swapper_inj_balance_after, expected_difference,
        "wrong amount of INJ was exchanged"
    );

    assert!(
        swapper_atom_balance_after >= exact_quantity_to_receive,
        "swapper got less than exact amount required -> expected: {} ATOM, actual: {} ATOM",
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg())
    );

    let one_percent_diff = exact_quantity_to_receive
        * (FPDecimal::must_from_str(max_diff_percentage.0) / FPDecimal::from(100u128));

    assert!(
        are_fpdecimals_approximately_equal(
            swapper_atom_balance_after,
            exact_quantity_to_receive,
            one_percent_diff,
        ),
        "swapper did not receive expected exact ATOM amount +/- {}% -> expected: {} ATOM, actual: {} ATOM, max diff: {} ATOM",
        max_diff_percentage.0,
        exact_quantity_to_receive.scaled(Decimals::Six.get_decimals().neg()),
        swapper_atom_balance_after.scaled(Decimals::Six.get_decimals().neg()),
        one_percent_diff.scaled(Decimals::Six.get_decimals().neg())
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

    // contract is allowed to earn extra 0.7 USDT from the swap of ~$8150 worth of INJ
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
