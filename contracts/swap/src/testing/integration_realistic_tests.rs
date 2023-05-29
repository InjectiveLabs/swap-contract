use cosmwasm_std::{coin, Addr};

use injective_test_tube::RunnerError::{ExecuteError, QueryError};
use injective_test_tube::{
    Account, Bank, Exchange, Gov, InjectiveTestApp, Module, RunnerError, RunnerResult, Wasm,
};

use injective_math::{round_to_min_tick, FPDecimal};
use injective_std::types::injective::exchange::v1beta1::{QuerySubaccountDepositsRequest, Subaccount};

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::testing::test_utils::{create_limit_order, create_realistic_limit_order, fund_account_with_some_inj, init_contract_and_get_address, init_contract_with_fee_recipient_and_get_address, launch_custom_spot_market, launch_spot_market, must_init_account_with_funds, pause_spot_market, query_all_bank_balances, query_bank_balance, set_route_and_assert_success, str_coin, Decimals, OrderSide, human_to_dec, dec_to_proto};

const ETH: &str = "eth";
const ATOM: &str = "atom";
const SOL: &str = "sol";
const USDT: &str = "usdt";
const USDC: &str = "usdc";
const INJ: &str = "inj";

const DEFAULT_TAKER_FEE: f64 = 0.001;
const DEFAULT_ATOMIC_MULTIPLIER: f64 = 2.5;
const DEFAULT_SELF_RELAYING_FEE_PART: f64 = 0.6;
const DEFAULT_RELAYER_SHARE: f64 = 1.0 - DEFAULT_SELF_RELAYING_FEE_PART;

#[test]
fn happy_path_two_hops_swap_realistic_scales() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, &Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, &Decimals::Eighteen),
            str_coin("1", ATOM, &Decimals::Six),
            str_coin("1_000_000", USDT, &Decimals::Six),
            str_coin("100_000", INJ, &Decimals::Eighteen),
        ],
    );

    let spot_market_1_id =
        launch_custom_spot_market(&exchange, &owner, ETH, USDT, dec_to_proto( FPDecimal::must_from_str("0.000000000000001")).as_str(), dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str());
    let spot_market_2_id =
        launch_custom_spot_market(&exchange, &owner, ATOM, USDT, dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(), dec_to_proto(FPDecimal::must_from_str("1000")).as_str());


    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("100_000", USDT, &Decimals::Six)]);
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

    let trader1 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

    let trader2 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

    let trader3 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

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
            str_coin("12", ETH, &Decimals::Eighteen),
            str_coin("5", INJ, &Decimals::Eighteen),
        ],
    );

    let query_result: FPDecimal = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetExecutionQuantity {
                source_denom: ETH.to_string(),
                to_denom: ATOM.to_string(),
                from_quantity: human_to_dec("12", &Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("2893.888", &Decimals::Six),
        "incorrect swap result estimate returned by query"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    println!("contract balances before: {:?}", contract_balances_before);

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(2800u128),
        },
        &[str_coin("12", ETH, &Decimals::Eighteen)],
        &swapper,
    ).unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        human_to_dec("2893.888", &Decimals::Six),
        "swapper did not receive expected amount"
    );

    let subacc_deps = exchange.query_subaccount_deposits(&QuerySubaccountDepositsRequest{ subaccount_id: "".to_string(), subaccount: Some(Subaccount { trader: contr_addr.to_string(), subaccount_nonce: 0 }) });
    println!("subacc deps: {:?}", subacc_deps);
    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());
    println!("contract balances after: {:?}", contract_balances_after)  ;
    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let atom_amount_below_min_tick_size = FPDecimal::must_from_str("0.000685");
    let mut dust_value = atom_amount_below_min_tick_size * human_to_dec("830", &Decimals::Six);
    let fee_refund = dust_value * FPDecimal::must_from_str(&format!(
        "{}",
        DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_SELF_RELAYING_FEE_PART));

    dust_value += fee_refund;

    let expected_contract_usdt_balance = FPDecimal::must_from_str(contract_balances_before[0].amount.as_str()) + dust_value;
    let actual_contract_balance = FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());
    let contract_balance_diff = expected_contract_usdt_balance.abs_diff(&actual_contract_balance);

    assert!(
        human_to_dec("0.001", &Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
    );
}

#[test]
fn happy_path_two_hops_swap_realistic_values() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let exchange = Exchange::new(&app);
    let bank = Bank::new(&app);

    let _signer = must_init_account_with_funds(&app, &[str_coin("1", INJ, &Decimals::Eighteen)]);

    let _validator = app
        .get_first_validator_signing_account(INJ.to_string(), 1.2f64)
        .unwrap();
    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, &Decimals::Eighteen),
            str_coin("1", ATOM, &Decimals::Six),
            str_coin("1_000", USDT, &Decimals::Six),
            str_coin("10_000", INJ, &Decimals::Eighteen),
        ],
    );

    let spot_market_1_id =
        launch_custom_spot_market(&exchange, &owner, ETH, USDT, dec_to_proto( FPDecimal::must_from_str("0.000000000000001")).as_str(), dec_to_proto(FPDecimal::must_from_str("1000000000000000")).as_str());
    let spot_market_2_id =
        launch_custom_spot_market(&exchange, &owner, ATOM, USDT, dec_to_proto(FPDecimal::must_from_str("0.001")).as_str(), dec_to_proto(FPDecimal::must_from_str("1000")).as_str());


    let contr_addr =
        init_contract_and_get_address(&wasm, &owner, &[str_coin("100", USDT, &Decimals::Six)]);
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

    let trader1 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

    let trader2 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

    let trader3 = must_init_account_with_funds(
        &app,
        &[
            str_coin("10_000", ETH, &Decimals::Eighteen),
            str_coin("10_000_000", USDT, &Decimals::Six),
            str_coin("1_000_000", ATOM, &Decimals::Six),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

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
            str_coin(eth_to_swap, ETH, &Decimals::Eighteen),
            str_coin("1", INJ, &Decimals::Eighteen),
        ],
    );

    let query_result: FPDecimal = wasm
        .query(
            &contr_addr,
            &QueryMsg::GetExecutionQuantity {
                source_denom: ETH.to_string(),
                to_denom: ATOM.to_string(),
                from_quantity: human_to_dec(eth_to_swap, &Decimals::Eighteen),
            },
        )
        .unwrap();

    assert_eq!(
        query_result,
        human_to_dec("906.262", &Decimals::Six),
        "incorrect swap result estimate returned by query"
    );

    let contract_balances_before = query_all_bank_balances(&bank, &contr_addr);
    assert_eq!(
        contract_balances_before.len(),
        1,
        "wrong number of denoms in contract balances"
    );
    println!("contract balances before: {:?}", contract_balances_before);

    wasm.execute(
        &contr_addr,
        &ExecuteMsg::Swap {
            target_denom: ATOM.to_string(),
            min_quantity: FPDecimal::from(906u128),
        },
        &[str_coin(eth_to_swap, ETH, &Decimals::Eighteen)],
        &swapper,
    ).unwrap();

    let from_balance = query_bank_balance(&bank, ETH, swapper.address().as_str());
    let to_balance = query_bank_balance(&bank, ATOM, swapper.address().as_str());
    assert_eq!(
        from_balance,
        FPDecimal::zero(),
        "some of the original amount wasn't swapped"
    );
    assert_eq!(
        to_balance,
        human_to_dec("906.262", &Decimals::Six),
        "swapper did not receive expected amount"
    );

    let subacc_deps = exchange.query_subaccount_deposits(&QuerySubaccountDepositsRequest{ subaccount_id: "".to_string(), subaccount: Some(Subaccount { trader: contr_addr.to_string(), subaccount_nonce: 0 }) });
    println!("subacc deps: {:?}", subacc_deps);
    let contract_balances_after = query_all_bank_balances(&bank, contr_addr.as_str());

    assert_eq!(
        contract_balances_after.len(),
        1,
        "wrong number of denoms in contract balances"
    );

    let atom_amount_below_min_tick_size = FPDecimal::must_from_str("0.0005463");
    let mut dust_value = atom_amount_below_min_tick_size * human_to_dec("8.89", &Decimals::Six);

    let fee_refund = dust_value * FPDecimal::must_from_str(&format!(
        "{}",
        DEFAULT_TAKER_FEE * DEFAULT_ATOMIC_MULTIPLIER * DEFAULT_SELF_RELAYING_FEE_PART));

    dust_value += fee_refund;

    let expected_contract_usdt_balance = FPDecimal::must_from_str(contract_balances_before[0].amount.as_str()) + dust_value;
    let actual_contract_balance = FPDecimal::must_from_str(contract_balances_after[0].amount.as_str());
    let contract_balance_diff = expected_contract_usdt_balance - actual_contract_balance;

    // here the actual difference is 0.000067 USDT, which we attribute differences between decimal precision of Rust/Go and Google Sheets
    assert!(
        human_to_dec("0.0001", &Decimals::Six) - contract_balance_diff > FPDecimal::zero(),
        "contract balance has changed too much after swap"
    );
}
