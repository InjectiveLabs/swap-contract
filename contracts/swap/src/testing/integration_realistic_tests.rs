use injective_test_tube::{Account, Bank, Exchange, InjectiveTestApp, Module, RunnerResult, Wasm};

use injective_math::FPDecimal;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::testing::test_utils::{
    create_realistic_limit_order, dec_to_proto, human_to_dec, init_contract_and_get_address,
    init_default_validator_account, init_rich_account, launch_custom_spot_market,
    must_init_account_with_funds, query_all_bank_balances, query_bank_balance,
    set_route_and_assert_success, str_coin, Decimals, OrderSide, ATOM, DEFAULT_ATOMIC_MULTIPLIER,
    DEFAULT_SELF_RELAYING_FEE_PART, DEFAULT_TAKER_FEE, ETH, INJ, USDT,
};

/*
   This test suite focuses on using using realistic values both for spot markets and for orders.
   ATOM/USDT market parameters was taken from mainnet. ETH/USDT market parameters in reality
   mirror INJ/USDT spot market on mainnet (we did not want to use INJ/USDT market so that we don't
   mix balances changes coming from swap with those related to gas payment for contract execution).

   Hardcoded values used in these tests come from the second tab of this spreadsheet:
   https://docs.google.com/spreadsheets/d/1-0epjX580nDO_P2mm1tSjhvjJVppsvrO1BC4_wsBeyA/edit?usp=sharing
*/

#[test]
fn happy_path_two_hops_swap_realistic_scales() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, Decimals::Eighteen)]);
    let _validator = init_default_validator_account(&app);
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000_000", USDT, Decimals::Six),
            str_coin("100_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("100_000", USDT, Decimals::Six)]);
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

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "201_000",
        "5",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "195_000",
        "4",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "192_000",
        "3",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "800",
        "800",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_2_id,
        OrderSide::Sell,
        "810",
        "800",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader3,
        &spot_market_2_id,
        OrderSide::Sell,
        "820",
        "800",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "830",
        "800",
        Decimals::Six,
        Decimals::Six,
    );

    app.increase_time(1);

    let swapper = must_init_account_with_funds(
        &app,
        &[
            str_coin("12", ETH, Decimals::Eighteen),
            str_coin("5", INJ, Decimals::Eighteen),
        ],
    );

    let query_result: FPDecimal = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetExecutionQuantity {
                source_denom: ETH.to_string(),
                to_denom: ATOM.to_string(),
                from_quantity: human_to_dec("12", Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("2893.888", Decimals::Six),
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
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(2800u128),
        },
        &[str_coin("12", ETH, Decimals::Eighteen)],
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
        human_to_dec("2893.888", Decimals::Six),
        "swapper did not receive expected amount"
    );

    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let atom_amount_below_min_tick_size = FPDecimal::must_from_str("0.000685");
    let mut dust_value = atom_amount_below_min_tick_size * human_to_dec("830", Decimals::Six);
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
    let contract_balance_diff = expected_contract_usdt_balance.abs_diff(&actual_contract_balance);

    assert!(
        human_to_dec("0.001", Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
    );
}

#[test]
fn happy_path_two_hops_swap_realistic_values() {
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

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("1_000", USDT, Decimals::Six)]);
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

    // ETH-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "2107.2",
        "0.78",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1978",
        "1.23",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1966.66",
        "2.07",
        Decimals::Eighteen,
        Decimals::Six,
    );

    // ATOM-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.89",
        "197.89",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.93",
        "181.002",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader3,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.99",
        "203.12",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.01",
        "421.11",
        Decimals::Six,
        Decimals::Six,
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
            &QueryMsg::GetExecutionQuantity {
                source_denom: ETH.to_string(),
                to_denom: ATOM.to_string(),
                from_quantity: human_to_dec(eth_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("906.262", Decimals::Six),
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
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
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
        human_to_dec("906.262", Decimals::Six),
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
        human_to_dec("0.0001", Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
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

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("1_000", USDT, Decimals::Six)]);

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

    while counter < 100 {
        // ETH-USDT orders
        create_realistic_limit_order(
            &app,
            &trader1,
            &spot_market_1_id,
            OrderSide::Buy,
            "2107.2",
            "0.78",
            Decimals::Eighteen,
            Decimals::Six,
        );

        create_realistic_limit_order(
            &app,
            &trader2,
            &spot_market_1_id,
            OrderSide::Buy,
            "1978",
            "1.23",
            Decimals::Eighteen,
            Decimals::Six,
        );

        create_realistic_limit_order(
            &app,
            &trader2,
            &spot_market_1_id,
            OrderSide::Buy,
            "1966.66",
            "2.07",
            Decimals::Eighteen,
            Decimals::Six,
        );

        // ATOM-USDT orders
        create_realistic_limit_order(
            &app,
            &trader1,
            &spot_market_2_id,
            OrderSide::Sell,
            "8.89",
            "197.89",
            Decimals::Six,
            Decimals::Six,
        );

        create_realistic_limit_order(
            &app,
            &trader2,
            &spot_market_2_id,
            OrderSide::Sell,
            "8.93",
            "181.002",
            Decimals::Six,
            Decimals::Six,
        );

        create_realistic_limit_order(
            &app,
            &trader3,
            &spot_market_2_id,
            OrderSide::Sell,
            "8.99",
            "203.12",
            Decimals::Six,
            Decimals::Six,
        );

        create_realistic_limit_order(
            &app,
            &trader1,
            &spot_market_2_id,
            OrderSide::Sell,
            "9.01",
            "421.11",
            Decimals::Six,
            Decimals::Six,
        );

        app.increase_time(1);

        wasm.execute(
            &contr_addr,
            &ExecuteMsg::Swap {
                target_denom: ATOM.to_string(),
                min_quantity: FPDecimal::from(906u128),
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

    let contract_balance_diff =
        FPDecimal::must_from_str(contract_balances_after[0].amount.as_str())
            - FPDecimal::must_from_str(contract_balances_before[0].amount.as_str());

    // here the actual difference is 0.49 USDT after 100 executions, which we attribute differences between decimal precision of Rust/Go and Google Sheets
    println!(
        "contract balance diff: {}",
        contract_balance_diff / FPDecimal::must_from_str("1000000")
    );
    assert!(
        contract_balance_diff - human_to_dec("0.5", Decimals::Six) < FPDecimal::zero(),
        "contract balance has changed too much after swap"
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

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("51", USDT, Decimals::Six)]);
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

    // ETH-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "2107.2",
        "0.78",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1978",
        "1.23",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1966.66",
        "2.07",
        Decimals::Eighteen,
        Decimals::Six,
    );

    // ATOM-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.89",
        "197.89",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.93",
        "181.002",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader3,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.99",
        "203.12",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.01",
        "421.11",
        Decimals::Six,
        Decimals::Six,
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
            &QueryMsg::GetExecutionQuantity {
                source_denom: ETH.to_string(),
                to_denom: ATOM.to_string(),
                from_quantity: human_to_dec(eth_to_swap, Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("906.262", Decimals::Six),
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
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
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
        human_to_dec("906.262", Decimals::Six),
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
        human_to_dec("0.0001", Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
    );
}

/*
   This test shows that in some edge cases we calculate required funds differently than the chain does.
   When estimating balance hold for atomic market order chain doesn't take into account whether sender is
   also fee recipient, while we do. This leads to a situation where we estimate required funds to be
   lower than what's expected by the chain, which makes the swap fail.
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

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("44", USDT, Decimals::Six)]);
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

    // ETH-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "2107.2",
        "0.78",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1978",
        "1.23",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1966.66",
        "2.07",
        Decimals::Eighteen,
        Decimals::Six,
    );

    // ATOM-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.89",
        "197.89",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.93",
        "181.002",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader3,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.99",
        "203.12",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.01",
        "421.11",
        Decimals::Six,
        Decimals::Six,
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

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, Decimals::Eighteen)],
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
        human_to_dec("906.262", Decimals::Six),
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
        human_to_dec("0.0001", Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
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

    let spot_market_1_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ETH,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.000000000000001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str(),
    );
    let spot_market_2_id = launch_custom_spot_market(
        &exchange,
        &owner,
        ATOM,
        USDT,
        dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(),
        dec_to_proto(FPDecimal::must_from_str("1000")).as_str(),
    );

    // 50 USDT is just below the amount required to buy required ATOM amount
    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("50", USDT, Decimals::Six)]);
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

    // ETH-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_1_id,
        OrderSide::Buy,
        "2107.2",
        "0.78",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1978",
        "1.23",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_1_id,
        OrderSide::Buy,
        "1966.66",
        "2.07",
        Decimals::Eighteen,
        Decimals::Six,
    );

    // ATOM-USDT orders
    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.89",
        "197.89",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader2,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.93",
        "181.002",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader3,
        &spot_market_2_id,
        OrderSide::Sell,
        "8.99",
        "203.12",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        &app,
        &trader1,
        &spot_market_2_id,
        OrderSide::Sell,
        "9.01",
        "421.11",
        Decimals::Six,
        Decimals::Six,
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
        &QueryMsg::GetExecutionQuantity {
            source_denom: ETH.to_string(),
            to_denom: ATOM.to_string(),
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
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(906u128),
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
        contract_balances_before[0].amount, contract_balances_after[0].amount,
        "contract balance has changed after failed swap"
    );
}
