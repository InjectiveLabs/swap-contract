use crate::helpers::Scaled;

use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{coin, to_json_binary, Addr, Coin, ContractResult, OwnedDeps, QuerierResult, SystemError, SystemResult};
use injective_cosmwasm::{
    create_orderbook_response_handler, create_spot_multi_market_handler, get_default_subaccount_id_for_checked_address, inj_mock_deps,
    test_market_ids, HandlesMarketIdQuery, InjectiveQueryWrapper, MarketId, PriceLevel, QueryMarketAtomicExecutionFeeMultiplierResponse, SpotMarket,
    WasmMockQuerier, TEST_MARKET_ID_1, TEST_MARKET_ID_2,
};
use injective_math::FPDecimal;
use injective_std::{
    shim::{Any, Timestamp},
    types::{
        cosmos::{
            authz::v1beta1::{Grant, MsgGrant},
            bank::v1beta1::{QueryAllBalancesRequest, QueryBalanceRequest},
        },
        cosmwasm::wasm::v1::{AcceptedMessageKeysFilter, ContractExecutionAuthorization, ContractGrant, MaxCallsLimit},
        injective::exchange::v1beta1::{MsgCreateSpotLimitOrder, OrderInfo, OrderType, SpotOrder},
    },
};
use injective_test_tube::{Account, Authz, Bank, Exchange, InjectiveTestApp, Module, SigningAccount, Wasm};
use injective_testing::{
    test_tube::{exchange::launch_spot_market_custom, utils::store_code},
    utils::scale_price_quantity_spot_market,
};
use prost::Message;
use std::{collections::HashMap, str::FromStr};

use crate::{
    msg::{ExecuteMsg, FeeRecipient, InstantiateMsg},
    types::FPCoin,
};

pub const TEST_CONTRACT_ADDR: &str = "inj14hj2tavq8fpesdwxxcu44rty3hh90vhujaxlnz";
pub const TEST_USER_ADDR: &str = "inj1p7z8p649xspcey7wp5e4leqf7wa39kjjj6wja8";

pub const ETH: &str = "eth";
pub const ATOM: &str = "atom";
pub const USDT: &str = "usdt";
pub const USDC: &str = "usdc";
pub const INJ: &str = "inj";
pub const INJ_2: &str = "inj_2";
pub const NINJA: &str = "ninja";

pub const DEFAULT_TAKER_FEE: f64 = 0.001;
pub const DEFAULT_ATOMIC_MULTIPLIER: f64 = 2.5;
pub const DEFAULT_SELF_RELAYING_FEE_PART: f64 = 0.6;

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
#[repr(i32)]
pub enum Decimals {
    Eighteen = 18,
    Six = 6,
}

impl Decimals {
    pub fn get_decimals(&self) -> i32 {
        match self {
            Decimals::Eighteen => 18,
            Decimals::Six => 6,
        }
    }
}

// Helper function to create a PriceLevel
pub fn create_price_level(p: u128, q: u128) -> PriceLevel {
    PriceLevel {
        p: FPDecimal::from(p),
        q: FPDecimal::from(q),
    }
}

#[derive(PartialEq)]
pub enum MultiplierQueryBehavior {
    Success,
    Fail,
}

pub fn mock_deps_eth_inj(
    multiplier_query_behavior: MultiplierQueryBehavior,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier, InjectiveQueryWrapper> {
    inj_mock_deps(|querier| {
        let mut markets = HashMap::new();
        markets.insert(
            MarketId::new(TEST_MARKET_ID_1).unwrap(),
            create_mock_spot_market("eth", FPDecimal::must_from_str("0.001"), FPDecimal::must_from_str("0.001"), 0),
        );
        markets.insert(
            MarketId::new(TEST_MARKET_ID_2).unwrap(),
            create_mock_spot_market("inj", FPDecimal::must_from_str("0.001"), FPDecimal::must_from_str("0.001"), 1),
        );
        querier.spot_market_response_handler = create_spot_multi_market_handler(markets);

        let mut orderbooks = HashMap::new();
        let eth_buy_orderbook = vec![
            PriceLevel {
                p: 201000u128.into(),
                q: FPDecimal::from_str("5").unwrap(),
            },
            PriceLevel {
                p: 195000u128.into(),
                q: FPDecimal::from_str("4").unwrap(),
            },
            PriceLevel {
                p: 192000u128.into(),
                q: FPDecimal::from_str("3").unwrap(),
            },
        ];
        orderbooks.insert(MarketId::new(TEST_MARKET_ID_1).unwrap(), eth_buy_orderbook);

        let inj_sell_orderbook = vec![
            PriceLevel {
                p: 800u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 810u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 820u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 830u128.into(),
                q: 800u128.into(),
            },
        ];
        orderbooks.insert(MarketId::new(TEST_MARKET_ID_2).unwrap(), inj_sell_orderbook);

        querier.spot_market_orderbook_response_handler = create_orderbook_response_handler(orderbooks);

        if multiplier_query_behavior == MultiplierQueryBehavior::Fail {
            pub fn create_spot_error_multiplier_handler() -> Option<Box<dyn HandlesMarketIdQuery>> {
                struct Temp {}

                impl HandlesMarketIdQuery for Temp {
                    fn handle(&self, _: MarketId) -> QuerierResult {
                        SystemResult::Err(SystemError::Unknown {})
                    }
                }

                Some(Box::new(Temp {}))
            }

            querier.market_atomic_execution_fee_multiplier_response_handler = create_spot_error_multiplier_handler()
        } else {
            pub fn create_spot_ok_multiplier_handler() -> Option<Box<dyn HandlesMarketIdQuery>> {
                struct Temp {}

                impl HandlesMarketIdQuery for Temp {
                    fn handle(&self, _: MarketId) -> QuerierResult {
                        let response = QueryMarketAtomicExecutionFeeMultiplierResponse {
                            multiplier: FPDecimal::from_str("2.5").unwrap(),
                        };
                        SystemResult::Ok(ContractResult::from(to_json_binary(&response)))
                    }
                }

                Some(Box::new(Temp {}))
            }

            querier.market_atomic_execution_fee_multiplier_response_handler = create_spot_ok_multiplier_handler()
        }
    })
}

pub fn mock_realistic_deps_eth_atom(
    multiplier_query_behavior: MultiplierQueryBehavior,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier, InjectiveQueryWrapper> {
    inj_mock_deps(|querier| {
        let mut markets = HashMap::new();
        markets.insert(
            MarketId::new(TEST_MARKET_ID_1).unwrap(),
            create_mock_spot_market(
                "eth",
                FPDecimal::must_from_str("0.000000000000001"),
                FPDecimal::must_from_str("1000000000000000"),
                0,
            ),
        );
        markets.insert(
            MarketId::new(TEST_MARKET_ID_2).unwrap(),
            create_mock_spot_market("atom", FPDecimal::must_from_str("0.001"), FPDecimal::must_from_str("10000"), 1),
        );
        querier.spot_market_response_handler = create_spot_multi_market_handler(markets);

        let mut orderbooks = HashMap::new();
        let eth_buy_orderbook = vec![
            PriceLevel {
                p: FPDecimal::must_from_str("0.000000002107200000"),
                q: FPDecimal::from_str("784000000000000000.000000000000000000").unwrap(),
            },
            PriceLevel {
                p: FPDecimal::must_from_str("0.000000001978000000"),
                q: FPDecimal::from_str("1230000000000000000.000000000000000000").unwrap(),
            },
            PriceLevel {
                p: FPDecimal::must_from_str("0.000000001966660000"),
                q: FPDecimal::from_str("2070000000000000000.000000000000000000").unwrap(),
            },
        ];
        orderbooks.insert(MarketId::new(TEST_MARKET_ID_1).unwrap(), eth_buy_orderbook);

        let inj_sell_orderbook = vec![
            PriceLevel {
                p: 800u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 810u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 820u128.into(),
                q: 800u128.into(),
            },
            PriceLevel {
                p: 830u128.into(),
                q: 800u128.into(),
            },
        ];
        orderbooks.insert(MarketId::new(TEST_MARKET_ID_2).unwrap(), inj_sell_orderbook);

        querier.spot_market_orderbook_response_handler = create_orderbook_response_handler(orderbooks);

        if multiplier_query_behavior == MultiplierQueryBehavior::Fail {
            pub fn create_spot_error_multiplier_handler() -> Option<Box<dyn HandlesMarketIdQuery>> {
                struct Temp {}

                impl HandlesMarketIdQuery for Temp {
                    fn handle(&self, _: MarketId) -> QuerierResult {
                        SystemResult::Err(SystemError::Unknown {})
                    }
                }

                Some(Box::new(Temp {}))
            }

            querier.market_atomic_execution_fee_multiplier_response_handler = create_spot_error_multiplier_handler()
        } else {
            pub fn create_spot_ok_multiplier_handler() -> Option<Box<dyn HandlesMarketIdQuery>> {
                struct Temp {}

                impl HandlesMarketIdQuery for Temp {
                    fn handle(&self, _: MarketId) -> QuerierResult {
                        let response = QueryMarketAtomicExecutionFeeMultiplierResponse {
                            multiplier: FPDecimal::from_str("2.5").unwrap(),
                        };
                        SystemResult::Ok(ContractResult::from(to_json_binary(&response)))
                    }
                }

                Some(Box::new(Temp {}))
            }

            querier.market_atomic_execution_fee_multiplier_response_handler = create_spot_ok_multiplier_handler()
        }
    })
}

fn create_mock_spot_market(base: &str, min_price_tick_size: FPDecimal, min_quantity_tick_size: FPDecimal, idx: u32) -> SpotMarket {
    SpotMarket {
        ticker: format!("{base}usdt"),
        base_denom: base.to_string(),
        quote_denom: "usdt".to_string(),
        maker_fee_rate: FPDecimal::from_str("0.01").unwrap(),
        taker_fee_rate: FPDecimal::from_str("0.001").unwrap(),
        relayer_fee_share_rate: FPDecimal::from_str("0.4").unwrap(),
        market_id: test_market_ids()[idx as usize].clone(),
        status: injective_cosmwasm::MarketStatus::Active,
        min_price_tick_size,
        min_quantity_tick_size,
    }
}

pub fn launch_realistic_inj_usdt_spot_market(exchange: &Exchange<InjectiveTestApp>, signer: &SigningAccount) -> String {
    launch_spot_market_custom(
        exchange,
        signer,
        "INJ2/USDT".to_string(),
        INJ_2.to_string(),
        USDT.to_string(),
        "0.000000000000001".to_string(),
        "1000000000000000".to_string(),
    )
}

pub fn launch_realistic_weth_usdt_spot_market(exchange: &Exchange<InjectiveTestApp>, signer: &SigningAccount) -> String {
    launch_spot_market_custom(
        exchange,
        signer,
        "ETH/USDT".to_string(),
        ETH.to_string(),
        USDT.to_string(),
        "0.0000000000001".to_string(),
        "1000000000000000".to_string(),
    )
}

pub fn launch_realistic_atom_usdt_spot_market(exchange: &Exchange<InjectiveTestApp>, signer: &SigningAccount) -> String {
    launch_spot_market_custom(
        exchange,
        signer,
        "ATOM/USDT".to_string(),
        ATOM.to_string(),
        USDT.to_string(),
        "0.001".to_string(),
        "10000".to_string(),
    )
}

pub fn launch_realistic_usdt_usdc_spot_market(exchange: &Exchange<InjectiveTestApp>, signer: &SigningAccount) -> String {
    launch_spot_market_custom(
        exchange,
        signer,
        "USDT/USDC".to_string(),
        USDT.to_string(),
        USDC.to_string(),
        "0.0001".to_string(),
        "100".to_string(),
    )
}

pub fn launch_realistic_ninja_inj_spot_market(exchange: &Exchange<InjectiveTestApp>, signer: &SigningAccount) -> String {
    launch_spot_market_custom(
        exchange,
        signer,
        "NINJA/INJ2".to_string(),
        NINJA.to_string(),
        INJ_2.to_string(),
        "1000000".to_string(),
        "10000000".to_string(),
    )
}

pub fn create_realistic_eth_usdt_buy_orders_from_spreadsheet(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Buy,
        "2107.2",
        "0.78",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(app, trader2, market_id, OrderSide::Buy, "1978", "1.23", Decimals::Eighteen, Decimals::Six);

    create_realistic_limit_order(
        app,
        trader2,
        market_id,
        OrderSide::Buy,
        "1966.6",
        "2.07",
        Decimals::Eighteen,
        Decimals::Six,
    );
}

pub fn create_realistic_eth_usdt_sell_orders_from_spreadsheet(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
    trader3: &SigningAccount,
) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Sell,
        "2115.2",
        "0.5",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader2,
        market_id,
        OrderSide::Sell,
        "2118.9",
        "1.22",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader2,
        market_id,
        OrderSide::Sell,
        "2120.1",
        "1.72",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader3,
        market_id,
        OrderSide::Sell,
        "2121",
        "2.11",
        Decimals::Eighteen,
        Decimals::Six,
    );
}

pub fn create_realistic_inj_usdt_buy_orders_from_spreadsheet(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Buy,
        "8.91",
        "282.001",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader2,
        market_id,
        OrderSide::Buy,
        "8.78",
        "283.65",
        Decimals::Eighteen,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader2,
        market_id,
        OrderSide::Buy,
        "8.56",
        "407.607",
        Decimals::Eighteen,
        Decimals::Six,
    );
}

pub fn create_realistic_inj_usdt_sell_orders_from_spreadsheet(app: &InjectiveTestApp, market_id: &str, trader1: &SigningAccount) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Sell,
        "36",
        "2821.001",
        Decimals::Eighteen,
        Decimals::Six,
    );
}

pub fn create_realistic_atom_usdt_sell_orders_from_spreadsheet(
    app: &InjectiveTestApp,
    market_id: &str,
    trader1: &SigningAccount,
    trader2: &SigningAccount,
    trader3: &SigningAccount,
) {
    create_realistic_limit_order(app, trader1, market_id, OrderSide::Sell, "8.89", "197.89", Decimals::Six, Decimals::Six);

    create_realistic_limit_order(app, trader2, market_id, OrderSide::Sell, "8.93", "181.02", Decimals::Six, Decimals::Six);

    create_realistic_limit_order(app, trader3, market_id, OrderSide::Sell, "8.99", "203.12", Decimals::Six, Decimals::Six);

    create_realistic_limit_order(app, trader1, market_id, OrderSide::Sell, "9.01", "421.11", Decimals::Six, Decimals::Six);
}

pub fn create_realistic_usdt_usdc_both_side_orders(app: &InjectiveTestApp, market_id: &str, trader1: &SigningAccount) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Buy,
        "0.9982",
        "1000.001",
        Decimals::Six,
        Decimals::Six,
    );

    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Sell,
        "1.0008",
        "1000.001",
        Decimals::Six,
        Decimals::Six,
    );
}

// not really realistic yet
pub fn create_ninja_inj_both_side_orders(app: &InjectiveTestApp, market_id: &str, trader1: &SigningAccount) {
    create_realistic_limit_order(
        app,
        trader1,
        market_id,
        OrderSide::Sell,
        "0.00021",
        "1001000",
        Decimals::Six,
        Decimals::Eighteen,
    );
}

#[derive(PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[allow(clippy::too_many_arguments)]
pub fn create_realistic_limit_order(
    app: &InjectiveTestApp,
    trader: &SigningAccount,
    market_id: &str,
    order_side: OrderSide,
    price: &str,
    quantity: &str,
    base_decimals: Decimals,
    quote_decimals: Decimals,
) {
    let (price_to_send, quantity_to_send) = scale_price_quantity_spot_market(price, quantity, &(base_decimals as i32), &(quote_decimals as i32));

    let exchange = Exchange::new(app);
    exchange
        .create_spot_limit_order(
            MsgCreateSpotLimitOrder {
                sender: trader.address(),
                order: Some(SpotOrder {
                    market_id: market_id.to_string(),
                    order_info: Some(OrderInfo {
                        subaccount_id: get_default_subaccount_id_for_checked_address(&Addr::unchecked(trader.address())).to_string(),
                        fee_recipient: trader.address(),
                        price: price_to_send,
                        quantity: quantity_to_send,
                        cid: "".to_string(),
                    }),
                    order_type: if order_side == OrderSide::Buy {
                        OrderType::BuyAtomic.into()
                    } else {
                        OrderType::SellAtomic.into()
                    },
                    trigger_price: "".to_string(),
                }),
            },
            trader,
        )
        .unwrap();
}

pub fn init_self_relaying_contract_and_get_address(wasm: &Wasm<InjectiveTestApp>, owner: &SigningAccount, initial_balance: &[Coin]) -> String {
    let code_id = store_code(wasm, owner, "swap_contract".to_string());
    wasm.instantiate(
        code_id,
        &InstantiateMsg {
            fee_recipient: FeeRecipient::SwapContract,
            admin: Addr::unchecked(owner.address()),
        },
        Some(&owner.address()),
        Some("Swap"),
        initial_balance,
        owner,
    )
    .unwrap()
    .data
    .address
}

pub fn set_route_and_assert_success(
    wasm: &Wasm<InjectiveTestApp>,
    signer: &SigningAccount,
    contr_addr: &str,
    from_denom: &str,
    target_denom: &str,
    route: Vec<MarketId>,
) {
    wasm.execute(
        contr_addr,
        &ExecuteMsg::SetRoute {
            source_denom: from_denom.to_string(),
            target_denom: target_denom.to_string(),
            route,
        },
        &[],
        signer,
    )
    .unwrap();
}

pub fn must_init_account_with_funds(app: &InjectiveTestApp, initial_funds: &[Coin]) -> SigningAccount {
    app.init_account(initial_funds).unwrap()
}

pub fn query_all_bank_balances(bank: &Bank<InjectiveTestApp>, address: &str) -> Vec<injective_std::types::cosmos::base::v1beta1::Coin> {
    bank.query_all_balances(&QueryAllBalancesRequest {
        address: address.to_string(),
        resolve_denom: false,
        pagination: None,
    })
    .unwrap()
    .balances
}

pub fn query_bank_balance(bank: &Bank<InjectiveTestApp>, denom: &str, address: &str) -> FPDecimal {
    FPDecimal::from_str(
        bank.query_balance(&QueryBalanceRequest {
            address: address.to_string(),
            denom: denom.to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .as_str(),
    )
    .unwrap()
}

pub fn create_contract_authorization(
    app: &InjectiveTestApp,
    contract: String,
    granter: &SigningAccount,
    grantee: String,
    message: String,
    limit: u64,
    expiration: Option<Timestamp>,
) {
    let authz = Authz::new(app);

    let mut filter_buf = vec![];
    AcceptedMessageKeysFilter::encode(&AcceptedMessageKeysFilter { keys: vec![message] }, &mut filter_buf).unwrap();

    let mut limit_buf = vec![];
    MaxCallsLimit::encode(&MaxCallsLimit { remaining: limit }, &mut limit_buf).unwrap();

    let contract_grant = ContractGrant {
        contract,
        limit: Some(Any {
            type_url: MaxCallsLimit::TYPE_URL.to_string(),
            value: limit_buf,
        }),
        filter: Some(Any {
            type_url: AcceptedMessageKeysFilter::TYPE_URL.to_string(),
            value: filter_buf,
        }),
    };

    let mut buf = vec![];
    ContractExecutionAuthorization::encode(
        &ContractExecutionAuthorization {
            grants: vec![contract_grant],
        },
        &mut buf,
    )
    .unwrap();

    authz
        .grant(
            MsgGrant {
                granter: granter.address(),
                grantee,
                grant: Some(Grant {
                    authorization: Some(Any {
                        type_url: ContractExecutionAuthorization::TYPE_URL.to_string(),
                        value: buf.clone(),
                    }),
                    expiration,
                }),
            },
            granter,
        )
        .unwrap();
}

pub fn init_rich_account(app: &InjectiveTestApp) -> SigningAccount {
    must_init_account_with_funds(
        app,
        &[
            str_coin("100_000", ETH, Decimals::Eighteen),
            str_coin("100_000", ATOM, Decimals::Six),
            str_coin("100_000_000", USDT, Decimals::Six),
            str_coin("100_000_000", USDC, Decimals::Six),
            str_coin("100_000", INJ, Decimals::Eighteen),
            str_coin("100_000", INJ_2, Decimals::Eighteen),
            str_coin("100_000_000_000_000_000", NINJA, Decimals::Six),
        ],
    )
}

pub fn human_to_dec(raw_number: &str, decimals: Decimals) -> FPDecimal {
    FPDecimal::must_from_str(&raw_number.replace('_', "")).scaled(decimals.get_decimals())
}

pub fn human_to_proto(raw_number: &str, decimals: i32) -> String {
    FPDecimal::must_from_str(&raw_number.replace('_', "")).scaled(18 + decimals).to_string()
}

pub fn str_coin(human_amount: &str, denom: &str, decimals: Decimals) -> Coin {
    let scaled_amount = human_to_dec(human_amount, decimals);
    let as_int: u128 = scaled_amount.into();
    coin(as_int, denom)
}

mod tests {
    use crate::testing::test_utils::{human_to_dec, human_to_proto, scale_price_quantity_spot_market, Decimals};
    use injective_math::FPDecimal;

    #[test]
    fn it_converts_integer_to_dec() {
        let integer = "1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = FPDecimal::must_from_str("1000000000000000000");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");

        decimals = Decimals::Six;
        expected = FPDecimal::must_from_str("1000000");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 6 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_above_zero_to_dec() {
        let integer = "1.1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = FPDecimal::must_from_str("1100000000000000000");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");

        decimals = Decimals::Six;
        expected = FPDecimal::must_from_str("1100000");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 6 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_above_zero_with_max_precision_limit_of_18_to_dec() {
        let integer = "1.000000000000000001";
        let decimals = Decimals::Eighteen;
        let expected = FPDecimal::must_from_str("1000000000000000001");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_above_zero_with_max_precision_limit_of_6_to_dec() {
        let integer = "1.000001";
        let decimals = Decimals::Six;
        let expected = FPDecimal::must_from_str("1000001");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_below_zero_to_dec() {
        let integer = "0.1123";
        let mut decimals = Decimals::Eighteen;
        let mut expected = FPDecimal::must_from_str("112300000000000000");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");

        decimals = Decimals::Six;
        expected = FPDecimal::must_from_str("112300");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 6 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_below_zero_with_max_precision_limit_of_18_to_dec() {
        let integer = "0.000000000000000001";
        let decimals = Decimals::Eighteen;
        let expected = FPDecimal::must_from_str("1");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");
    }

    #[test]
    fn it_converts_decimals_below_zero_with_max_precision_limit_of_6_to_dec() {
        let integer = "0.000001";
        let decimals = Decimals::Six;
        let expected = FPDecimal::must_from_str("1");

        let actual = human_to_dec(integer, decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");
    }

    #[test]
    fn it_converts_integer_to_proto() {
        let integer = "1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = "1000000000000000000000000000000000000";

        let actual = human_to_proto(integer, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to proto");

        decimals = Decimals::Six;
        expected = "1000000000000000000000000";

        let actual = human_to_proto(integer, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert integer with 6 decimal to proto");
    }

    #[test]
    fn it_converts_decimal_above_zero_to_proto() {
        let number = "1.1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = "1100000000000000000000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 18 decimal to proto");

        decimals = Decimals::Six;
        expected = "1100000000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 6 decimal to proto");
    }

    #[test]
    fn it_converts_decimal_below_zero_to_proto() {
        let number = "0.1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = "100000000000000000000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 18 decimal to proto");

        decimals = Decimals::Six;
        expected = "100000000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 6 decimal to proto");
    }

    #[test]
    fn it_converts_decimal_below_zero_with_18_decimals_with_max_precision_to_proto() {
        let number = "0.000000000000000001";
        let decimals = Decimals::Eighteen;
        let expected = "1000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 18 decimal to proto");
    }

    #[test]
    #[should_panic]
    fn it_panics_when_converting_decimal_below_zero_with_18_decimals_with_too_high_precision_to_proto() {
        let number = "0.0000000000000000001";
        let decimals = Decimals::Eighteen;

        human_to_proto(number, decimals.get_decimals());
    }

    #[test]
    fn it_converts_decimal_below_zero_with_6_decimals_with_max_precision_to_proto() {
        let number = "0.000001";
        let decimals = Decimals::Six;
        let expected = "1000000000000000000";

        let actual = human_to_proto(number, decimals.get_decimals());
        assert_eq!(actual, expected, "failed to convert decimal with 6 decimal to proto");
    }

    #[test]
    fn it_converts_decimal_above_zero_with_0_precision_to_proto() {
        let number = "1.000001";
        let expected = "1000001000000000000";

        let actual = human_to_proto(number, 0);
        assert_eq!(actual, expected, "failed to convert decimal with 0 decimal to proto");
    }

    #[test]
    fn it_converts_decimal_below_zero_with_0_precision_to_proto() {
        let number = "0.000001";
        let expected = "1000000000000";

        let actual = human_to_proto(number, 0);
        assert_eq!(actual, expected, "failed to convert decimal with 0 decimal to proto");
    }

    #[test]
    fn it_scales_integer_values_correctly_for_inj_usdt() {
        let price = "1";
        let quantity = "1";

        let base_decimals = Decimals::Eighteen;
        let quote_decimals = Decimals::Six;

        let (scaled_price, scaled_quantity) = scale_price_quantity_spot_market(price, quantity, &(base_decimals as i32), &(quote_decimals as i32));

        // 1 => 1 * 10^6 - 10^18 => 0.000000000001000000 * 10^18 => 1000000
        assert_eq!(scaled_price, "1000000", "price was scaled incorrectly");
        // 1 => 1(10^18).(10^18) => 1000000000000000000000000000000000000
        assert_eq!(
            scaled_quantity, "1000000000000000000000000000000000000",
            "quantity was scaled incorrectly"
        );
    }

    #[test]
    fn it_scales_decimal_values_correctly_for_inj_usdt() {
        let price = "8.782";
        let quantity = "1.12";

        let base_decimals = Decimals::Eighteen;
        let quote_decimals = Decimals::Six;

        let (scaled_price, scaled_quantity) = scale_price_quantity_spot_market(price, quantity, &(base_decimals as i32), &(quote_decimals as i32));

        // 0.000000000008782000 * 10^18 = 8782000
        assert_eq!(scaled_price, "8782000", "price was scaled incorrectly");
        assert_eq!(
            scaled_quantity, "1120000000000000000000000000000000000",
            "quantity was scaled incorrectly"
        );
    }

    #[test]
    fn it_scales_integer_values_correctly_for_atom_usdt() {
        let price = "1";
        let quantity = "1";

        let base_decimals = Decimals::Six;
        let quote_decimals = Decimals::Six;

        let (scaled_price, scaled_quantity) = scale_price_quantity_spot_market(price, quantity, &(base_decimals as i32), &(quote_decimals as i32));

        // 1 => 1.(10^18) => 1000000000000000000
        assert_eq!(scaled_price, "1000000000000000000", "price was scaled incorrectly");
        // 1 => 1(10^6).(10^18) => 1000000000000000000000000
        assert_eq!(scaled_quantity, "1000000000000000000000000", "quantity was scaled incorrectly");
    }

    #[test]
    fn it_scales_decimal_values_correctly_for_atom_usdt() {
        let price = "1.129";
        let quantity = "1.62";

        let base_decimals = Decimals::Six;
        let quote_decimals = Decimals::Six;

        let (scaled_price, scaled_quantity) = scale_price_quantity_spot_market(price, quantity, &(base_decimals as i32), &(quote_decimals as i32));

        // 1.129 => 1.129(10^15) => 1129000000000000000
        assert_eq!(scaled_price, "1129000000000000000", "price was scaled incorrectly");
        // 1.62 => 1.62(10^4)(10^18) => 1000000000000000000000000
        assert_eq!(scaled_quantity, "1620000000000000000000000", "quantity was scaled incorrectly");
    }
}

pub fn are_fpdecimals_approximately_equal(first: FPDecimal, second: FPDecimal, max_diff: FPDecimal) -> bool {
    (first - second).abs() <= max_diff
}

pub fn assert_fee_is_as_expected(raw_fees: &mut [FPCoin], expected_fees: &mut [FPCoin], max_diff: FPDecimal) {
    assert_eq!(raw_fees.len(), expected_fees.len(), "Wrong number of fee denoms received");

    raw_fees.sort_by_key(|f| f.denom.clone());
    expected_fees.sort_by_key(|f| f.denom.clone());

    for (raw_fee, expected_fee) in raw_fees.iter().zip(expected_fees.iter()) {
        assert!(
            are_fpdecimals_approximately_equal(expected_fee.amount, raw_fee.amount, max_diff,),
            "Wrong amount of trx fee received. Expected: {}, Actual: {}, Max diff: {}",
            expected_fee.amount,
            raw_fee.amount,
            max_diff
        );
    }
}
