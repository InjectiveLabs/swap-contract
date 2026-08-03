use crate::{
    msg::{ExecuteMsg, FeeRecipient, InstantiateMsg},
    testing::test_utils::{set_route_and_assert_success, INJ},
};

use cosmwasm_std::{Addr, Coin};
use injective_cosmwasm::get_default_subaccount_id_for_checked_address;
use injective_math::FPDecimal;
use injective_std::types::{
    cosmos::{bank::v1beta1::QueryBalanceRequest, base::v1beta1::Coin as ProtoCoin},
    cosmwasm::wasm::v1::MsgExecuteContract,
    injective::exchange::v1beta1::{
        MsgCreateSpotLimitOrder, MsgCreateSpotLimitOrderResponse, MsgInstantSpotMarketLaunch, OrderInfo, OrderType, SpotOrder,
    },
};
use injective_test_tube::{cosmrs::Any, Account, Bank, Exchange, InjectiveTestApp, Module, Runner, SigningAccount, Wasm};
use injective_testing::{
    test_tube::{
        bank::send,
        exchange::{add_denom_notional_and_decimal, get_spot_market_id},
    },
    utils::{dec_to_proto, scale_price_quantity_spot_market},
};
use prost::Message;

const GF: &str = "ugf";
const USDT: &str = "usdt";
const BASE_DECIMALS: i32 = 18;
const QUOTE_DECIMALS: i32 = 6;
const CALLER_INPUT_USDT_ATOMS: u128 = 10_035_014;
const CONTRACT_SUPPORT_USDT_ATOMS: u128 = 4_000_000_000;
const ATTACKER_GF_ATOMS: u128 = 10_009_989_000_000_000_000_000;
const LOW_QUANTITY_ATOMS: u128 = 10_000_000_000_000_000_000_000;
const SELECTED_HIGH_QUANTITY_ATOMS: u128 = 30_000_000_000_000_000;
const PRICE_TICK_RAW: u128 = 1_000;
const LOW_PRICE_RAW: u128 = 1_000;
const HIGH_PRICE_RAW: u128 = 333_000;

#[test]
fn same_transaction_crafted_asks_cannot_spend_contract_support() {
    assert_crafted_book_crosses_the_next_exact_price_tick();

    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);
    let owner = app
        .init_account_decimals(
            &[
                Coin::new(1u128, GF),
                Coin::new(10_000_000_000_000_000_000_000u128, INJ),
                Coin::new(5_000_000_000u128, USDT),
            ],
            &[18, 18, 6],
        )
        .unwrap();
    let attacker = app
        .init_account(&[
            Coin::new(ATTACKER_GF_ATOMS, GF),
            Coin::new(1_000_000_000_000_000_000_000u128, INJ),
            Coin::new(CALLER_INPUT_USDT_ATOMS, USDT),
        ])
        .unwrap();
    let market_id = setup_market(&app, &owner);
    let release_wasm_path = format!("{}/../../artifacts/swap_contract.wasm", env!("CARGO_MANIFEST_DIR"));
    let release_wasm = std::fs::read(release_wasm_path).expect("build the optimized release artifact before running TestTube tests");
    let code_id = wasm.store_code(&release_wasm, None, &owner).unwrap().data.code_id;
    let contract = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                fee_recipient: FeeRecipient::Address(Addr::unchecked(owner.address())),
                admin: Addr::unchecked(owner.address()),
            },
            Some(&owner.address()),
            Some("BB-311 regression"),
            &[Coin::new(CONTRACT_SUPPORT_USDT_ATOMS, USDT)],
            &owner,
        )
        .unwrap()
        .data
        .address;

    set_route_and_assert_success(&wasm, &owner, &contract, USDT, GF, vec![market_id.as_str().into()]);

    let contract_usdt_before = bank_balance(&bank, &contract, USDT);
    let attacker_usdt_before = bank_balance(&bank, &attacker.address(), USDT);
    let low_ask = sell_post_only_order(&attacker, &market_id, "0.001", "10000.000");
    let high_ask = sell_post_only_order(&attacker, &market_id, "0.333", "9.989");
    let swap = MsgExecuteContract {
        sender: attacker.address(),
        contract: contract.clone(),
        msg: serde_json_wasm::to_vec(&ExecuteMsg::SwapMinOutput {
            target_denom: GF.to_string(),
            min_output_quantity: FPDecimal::ONE,
        })
        .unwrap(),
        funds: vec![ProtoCoin {
            denom: USDT.to_string(),
            amount: CALLER_INPUT_USDT_ATOMS.to_string(),
        }],
    };

    let response = app
        .execute_multiple_raw::<MsgCreateSpotLimitOrderResponse>(
            vec![
                proto_any(&low_ask, "/injective.exchange.v1beta1.MsgCreateSpotLimitOrder"),
                proto_any(&high_ask, "/injective.exchange.v1beta1.MsgCreateSpotLimitOrder"),
                proto_any(&swap, "/cosmwasm.wasm.v1.MsgExecuteContract"),
            ],
            &attacker,
        )
        .unwrap();

    let contract_usdt_after = bank_balance(&bank, &contract, USDT);
    let attacker_usdt_after = bank_balance(&bank, &attacker.address(), USDT);
    let delivered_gf: u128 = response
        .events
        .iter()
        .find(|event| event.ty.contains("atomic_swap_execution"))
        .and_then(|event| event.attributes.iter().find(|attribute| attribute.key == "swap_final_amount"))
        .unwrap()
        .value
        .parse()
        .unwrap();

    assert!(contract_usdt_after >= contract_usdt_before, "pre-existing USDT support must not decrease");
    assert!(
        attacker_usdt_after <= attacker_usdt_before,
        "the maker/caller must not profit in quote funds"
    );
    assert!(
        delivered_gf < LOW_QUANTITY_ATOMS,
        "the rounded average must not size a buy for the complete low ask"
    );
}

fn assert_crafted_book_crosses_the_next_exact_price_tick() {
    let exact_notional = LOW_PRICE_RAW * LOW_QUANTITY_ATOMS + HIGH_PRICE_RAW * SELECTED_HIGH_QUANTITY_ATOMS;
    let exact_quantity = LOW_QUANTITY_ATOMS + SELECTED_HIGH_QUANTITY_ATOMS;
    let truncated_average = exact_notional / exact_quantity;
    let exact_remainder = exact_notional % exact_quantity;
    let tick_denominator = exact_quantity * PRICE_TICK_RAW;
    let exact_tick_ceiling = exact_notional.div_ceil(tick_denominator) * PRICE_TICK_RAW;

    assert_eq!(truncated_average, PRICE_TICK_RAW);
    assert_ne!(exact_remainder, 0);
    assert_eq!(exact_tick_ceiling, PRICE_TICK_RAW * 2);
}

fn setup_market(app: &InjectiveTestApp, owner: &SigningAccount) -> String {
    let validator = app.get_first_validator_signing_account(INJ.to_string(), 1.2).unwrap();
    send(&Bank::new(app), "1000000000000000000000", INJ, owner, &validator);
    add_denom_notional_and_decimal(app, &validator, GF.to_string(), "1".to_string(), BASE_DECIMALS as u64);
    add_denom_notional_and_decimal(app, &validator, USDT.to_string(), "1".to_string(), QUOTE_DECIMALS as u64);

    let exchange = Exchange::new(app);
    let ticker = "GF/USDT-BB311".to_string();
    exchange
        .instant_spot_market_launch(
            MsgInstantSpotMarketLaunch {
                sender: owner.address(),
                ticker: ticker.clone(),
                base_denom: GF.to_string(),
                quote_denom: USDT.to_string(),
                min_price_tick_size: dec_to_proto(FPDecimal::must_from_str("0.000000000000001")),
                min_quantity_tick_size: dec_to_proto(FPDecimal::must_from_str("1000000000000000")),
                min_notional: dec_to_proto(FPDecimal::ONE),
                base_decimals: BASE_DECIMALS as u32,
                quote_decimals: QUOTE_DECIMALS as u32,
            },
            owner,
        )
        .unwrap();

    get_spot_market_id(&exchange, ticker)
}

fn sell_post_only_order(trader: &SigningAccount, market_id: &str, price: &str, quantity: &str) -> MsgCreateSpotLimitOrder {
    let (price, quantity) = scale_price_quantity_spot_market(price, quantity, &BASE_DECIMALS, &QUOTE_DECIMALS);

    MsgCreateSpotLimitOrder {
        sender: trader.address(),
        order: Some(SpotOrder {
            market_id: market_id.to_string(),
            order_info: Some(OrderInfo {
                subaccount_id: get_default_subaccount_id_for_checked_address(&Addr::unchecked(trader.address())).to_string(),
                fee_recipient: trader.address(),
                price,
                quantity,
                cid: String::new(),
            }),
            order_type: OrderType::SellPo.into(),
            trigger_price: String::new(),
        }),
    }
}

fn proto_any<M: Message>(message: &M, type_url: &str) -> Any {
    let mut value = Vec::new();
    message.encode(&mut value).unwrap();
    Any {
        type_url: type_url.to_string(),
        value,
    }
}

fn bank_balance(bank: &Bank<InjectiveTestApp>, address: &str, denom: &str) -> u128 {
    bank.query_balance(&QueryBalanceRequest {
        address: address.to_string(),
        denom: denom.to_string(),
    })
    .unwrap()
    .balance
    .map(|balance| balance.amount.parse().unwrap())
    .unwrap_or_default()
}
