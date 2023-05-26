use std::collections::HashMap;
use std::str::FromStr;

use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{Addr, Coin, coin, OwnedDeps, QuerierResult, SystemError, SystemResult, Uint128};
use injective_std::shim::Any;
use injective_std::types::cosmos::bank::v1beta1::{
    MsgSend, QueryAllBalancesRequest, QueryBalanceRequest,
};
use injective_std::types::cosmos::base::v1beta1::Coin as TubeCoin;
use injective_std::types::cosmos::gov::v1beta1::{MsgSubmitProposal, MsgVote};
use injective_std::types::injective::exchange;
use injective_std::types::injective::exchange::v1beta1::{
    MsgCreateSpotLimitOrder, MsgInstantSpotMarketLaunch, OrderInfo, OrderType,
    QuerySpotMarketsRequest, SpotMarketParamUpdateProposal, SpotOrder,
};
use injective_test_tube::{
    Account, Bank, Exchange, Gov, InjectiveTestApp, Module, SigningAccount, Wasm,
};

use injective_cosmwasm::{
    create_mock_spot_market, create_orderbook_response_handler, create_spot_multi_market_handler,
    get_default_subaccount_id_for_checked_address, inj_mock_deps, HandlesMarketIdQuery,
    InjectiveQueryWrapper, MarketId, PriceLevel, WasmMockQuerier, TEST_MARKET_ID_1,
    TEST_MARKET_ID_2,
};
use injective_math::FPDecimal;
use prost::Message;

use crate::msg::{ExecuteMsg, FeeRecipient, InstantiateMsg};

pub const TEST_CONTRACT_ADDR: &str = "inj14hj2tavq8fpesdwxxcu44rty3hh90vhujaxlnz";
pub const TEST_USER_ADDR: &str = "inj1p7z8p649xspcey7wp5e4leqf7wa39kjjj6wja8";

#[derive(PartialEq, Eq, Debug)]
pub enum Decimals {
    Eighteen,
    Twelve,
    Six,
    Zero,
}

impl Decimals {
    pub fn get_decimals(&self) -> usize {
        match self {
            Decimals::Eighteen => 18,
            Decimals::Twelve => 12,
            Decimals::Six => 6,
            Decimals::Zero => 0,
        }
    }

    pub fn get_right_padding_zeroes(&self) -> String {
        match self {
            Decimals::Eighteen => "000000000000000000".to_string(),
            Decimals::Twelve => "000000000000".to_string(),
            Decimals::Six => "000000".to_string(),
            Decimals::Zero => "".to_string(),
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
pub enum MultiplierQueryBehaviour {
    Success,
    Fail,
}

pub fn mock_deps_eth_inj(
    multiplier_query_behaviour: MultiplierQueryBehaviour,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier, InjectiveQueryWrapper> {
    inj_mock_deps(|querier| {
        let mut markets = HashMap::new();
        markets.insert(
            MarketId::new(TEST_MARKET_ID_1).unwrap(),
            create_mock_spot_market("eth", 0),
        );
        markets.insert(
            MarketId::new(TEST_MARKET_ID_2).unwrap(),
            create_mock_spot_market("inj", 1),
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

        querier.spot_market_orderbook_response_handler =
            create_orderbook_response_handler(orderbooks);

        if multiplier_query_behaviour == MultiplierQueryBehaviour::Fail {
            pub fn create_spot_error_multi_market_handler() -> Option<Box<dyn HandlesMarketIdQuery>>
            {
                struct Temp {}

                impl HandlesMarketIdQuery for Temp {
                    fn handle(&self, _: MarketId) -> QuerierResult {
                        SystemResult::Err(SystemError::Unknown {})
                    }
                }

                Some(Box::new(Temp {}))
            }

            querier.market_atomic_execution_fee_multiplier_response_handler =
                create_spot_error_multi_market_handler()
        }
    })
}

pub fn wasm_file(contract_name: String) -> String {
    let arch = std::env::consts::ARCH;
    let artifacts_dir =
        std::env::var("ARTIFACTS_DIR_PATH").unwrap_or_else(|_| "artifacts".to_string());
    let snaked_name = contract_name.replace('-', "_");
    format!("../../{artifacts_dir}/{snaked_name}-{arch}.wasm")
}

pub fn store_code(
    wasm: &Wasm<InjectiveTestApp>,
    owner: &SigningAccount,
    contract_name: String,
) -> u64 {
    let wasm_byte_code = std::fs::read(wasm_file(contract_name)).unwrap();
    wasm.store_code(&wasm_byte_code, None, owner)
        .unwrap()
        .data
        .code_id
}

pub fn launch_spot_market(
    exchange: &Exchange<InjectiveTestApp>,
    signer: &SigningAccount,
    base: &str,
    quote: &str,
) -> String {
    let ticker = format!("{}/{}", base, quote);
    exchange
        .instant_spot_market_launch(
            MsgInstantSpotMarketLaunch {
                sender: signer.address(),
                ticker: ticker.clone(),
                base_denom: base.to_string(),
                quote_denom: quote.to_string(),
                min_price_tick_size: "1_000_000_000_000_000".to_owned(),
                min_quantity_tick_size: "1_000_000_000_000_000".to_owned(),
            },
            signer,
        )
        .unwrap();

    get_spot_market_id(exchange, ticker)
}

pub fn launch_custom_spot_market(
    exchange: &Exchange<InjectiveTestApp>,
    signer: &SigningAccount,
    base: &str,
    quote: &str,
    min_price_tick_size: &str,
    min_quantity_tick_size: &str,
) -> String {
    let ticker = format!("{}/{}", base, quote);
    exchange
        .instant_spot_market_launch(
            MsgInstantSpotMarketLaunch {
                sender: signer.address(),
                ticker: ticker.clone(),
                base_denom: base.to_string(),
                quote_denom: quote.to_string(),
                min_price_tick_size: min_price_tick_size.to_string(),
                min_quantity_tick_size: min_quantity_tick_size.to_string(),
            },
            signer,
        )
        .unwrap();

    get_spot_market_id(exchange, ticker)
}

pub fn get_spot_market_id(exchange: &Exchange<InjectiveTestApp>, ticker: String) -> String {
    let spot_markets = exchange
        .query_spot_markets(&QuerySpotMarketsRequest {
            status: "Active".to_string(),
            market_ids: vec![],
        })
        .unwrap()
        .markets;

    let market = spot_markets.iter().find(|m| m.ticker == ticker).unwrap();

    market.market_id.to_string()
}

#[derive(PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

pub fn create_limit_order(
    app: &InjectiveTestApp,
    trader: &SigningAccount,
    market_id: &str,
    order_side: OrderSide,
    price: u128,
    quantity: u32,
) {
    let exchange = Exchange::new(app);
    exchange
        .create_spot_limit_order(
            MsgCreateSpotLimitOrder {
                sender: trader.address(),
                order: Some(SpotOrder {
                    market_id: market_id.to_string(),
                    order_info: Some(OrderInfo {
                        subaccount_id: get_default_subaccount_id_for_checked_address(
                            &Addr::unchecked(trader.address()),
                        )
                            .to_string(),
                        fee_recipient: trader.address(),
                        price: format!("{}000000000000000000", price),
                        quantity: format!("{}000000000000000000", quantity),
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
    // base: 6
    // quote: 6
    // price: 10 -> 10
    // -----
    // base: 18
    // quote: 6
    // price: 8 -> 0.000000000008
    let get_scaled_integer_price = |raw_number: &str, base_decimals: &Decimals, quote_decimals: &Decimals| -> String {
        let number = raw_number.replace('_', "");
        let required_shift_to_zero = base_decimals.get_decimals() - quote_decimals.get_decimals();
        if required_shift_to_zero == 0 {
            return number.to_string();
        }

        let required_shift = required_shift_to_zero - number.len();
        if required_shift > 0 {
            format!("0.{}{number}", "0".repeat(required_shift))
        } else {
            format!("0.{number}")
        }
    };

    // get_scaled_integer_price(price, &base_decimals, &quote_decimals);
    let get_scaled_spot_price = |raw_number: &str, base_decimals: &Decimals, quote_decimals: &Decimals| -> String {
        let number = raw_number.replace('_', "");
        let has_decimal_fraction = number.contains(".");

        let generate_left_padding_zeroes = |number: &str, base_decimals: &Decimals, quote_decimals: &Decimals| -> String {
            let decimals_to_pad = base_decimals.get_decimals() - quote_decimals.get_decimals() - number.len();
            "0".repeat(decimals_to_pad)
        };

        let generate_right_padding_zeroes = |number: &str, base_decimals: &Decimals, quote_decimals: &Decimals| -> String {
            // let decimals_to_pad = base_decimals.get_decimals() - quote_decimals.get_decimals() - number.len();
            "0".repeat(quote_decimals.get_decimals())
        };

        if !has_decimal_fraction {
            return format!("{}{}", number, generate_right_padding_zeroes(number.as_str(), base_decimals, quote_decimals));
        }

        let separated: Vec<&str> = number.split_terminator('.').collect();
        if separated.len() != 2 {
            panic!("Invalid number format");
        }

        if separated[1].len() > base_decimals.get_decimals() {
            panic!("Decimal precision is too high");
        }

        let is_below_zero = number.chars().nth(0).unwrap().to_string() == "0";
        if is_below_zero {
            // take only decimal fraction and pad with zeros
            let left_zeros = generate_left_padding_zeroes(separated[1], base_decimals, quote_decimals);
            let decimal_padded = &format!("{}{}", separated[1], left_zeros);
            return decimal_padded.to_string();
        }

        // take integer and decimal fraction and pad with zeros
        let clean_number = &format!("{}{}", separated[0], separated[1]);
        let left_zeros = generate_left_padding_zeroes(clean_number, base_decimals, quote_decimals);
        format!("{}{}", left_zeros, clean_number)
    };

    // println!("price: {}", price);
    let mut price_to_send = get_scaled_integer_price(price, &base_decimals, &quote_decimals);
    // println!("price_to_send_raw: {}", price_to_send);

    let price_decimal_shift = &base_decimals.get_decimals() - &quote_decimals.get_decimals();
    price_to_send = human_to_proto(price_to_send.as_str(), price_decimal_shift);
    // println!("price_to_send: {}", price_to_send);
    let quantity_to_send = human_to_proto(quantity, base_decimals.get_decimals());
    // println!("quantity_to_send: {}", quantity_to_send);

    let exchange = Exchange::new(app);

    exchange
        .create_spot_limit_order(
            MsgCreateSpotLimitOrder {
                sender: trader.address(),
                order: Some(SpotOrder {
                    market_id: market_id.to_string(),
                    order_info: Some(OrderInfo {
                        subaccount_id: get_default_subaccount_id_for_checked_address(
                            &Addr::unchecked(trader.address()),
                        )
                            .to_string(),
                        fee_recipient: trader.address(),
                        price: price_to_send,
                        quantity: quantity_to_send,
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

pub fn init_contract_and_get_address(
    wasm: &Wasm<InjectiveTestApp>,
    owner: &SigningAccount,
    initial_balance: &[Coin],
) -> String {
    let code_id = store_code(wasm, owner, "helix_converter".to_string());
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

pub fn init_contract_with_fee_recipient_and_get_address(
    wasm: &Wasm<InjectiveTestApp>,
    owner: &SigningAccount,
    initial_balance: &[Coin],
    fee_recipient: &SigningAccount,
) -> String {
    let code_id = store_code(wasm, owner, "helix_converter".to_string());
    wasm.instantiate(
        code_id,
        &InstantiateMsg {
            fee_recipient: FeeRecipient::Address(Addr::unchecked(fee_recipient.address())),
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
    to_denom: &str,
    route: Vec<MarketId>,
) {
    wasm.execute(
        contr_addr,
        &ExecuteMsg::SetRoute {
            source_denom: from_denom.to_string(),
            target_denom: to_denom.to_string(),
            route,
        },
        &[],
        signer,
    )
        .unwrap();
}

pub fn must_init_account_with_funds(
    app: &InjectiveTestApp,
    initial_funds: &[Coin],
) -> SigningAccount {
    app.init_account(initial_funds).unwrap()
}

pub fn query_all_bank_balances(
    bank: &Bank<InjectiveTestApp>,
    address: &str,
) -> Vec<injective_std::types::cosmos::base::v1beta1::Coin> {
    bank.query_all_balances(&QueryAllBalancesRequest {
        address: address.to_string(),
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

pub fn pause_spot_market(
    gov: &Gov<InjectiveTestApp>,
    market_id: &str,
    proposer: &SigningAccount,
    validator: &SigningAccount,
) {
    pass_spot_market_params_update_proposal(
        gov,
        &SpotMarketParamUpdateProposal {
            title: format!("Set market {market_id} status to paused"),
            description: format!("Set market {market_id} status to paused"),
            market_id: market_id.to_string(),
            maker_fee_rate: "".to_string(),
            taker_fee_rate: "".to_string(),
            relayer_fee_share_rate: "".to_string(),
            min_price_tick_size: "".to_string(),
            min_quantity_tick_size: "".to_string(),
            status: 2,
        },
        proposer,
        validator,
    )
}

pub fn pass_spot_market_params_update_proposal(
    gov: &Gov<InjectiveTestApp>,
    proposal: &SpotMarketParamUpdateProposal,
    proposer: &SigningAccount,
    validator: &SigningAccount,
) {
    let mut buf = vec![];
    exchange::v1beta1::SpotMarketParamUpdateProposal::encode(proposal, &mut buf).unwrap();

    println!("submitting proposal: {:?}", proposal);
    let submit_response = gov.submit_proposal(
        MsgSubmitProposal {
            content: Some(Any {
                type_url: "/injective.exchange.v1beta1.SpotMarketParamUpdateProposal".to_string(),
                value: buf,
            }),
            initial_deposit: vec![TubeCoin {
                amount: "100000000000000000000".to_string(),
                denom: "inj".to_string(),
            }],
            proposer: proposer.address(),
        },
        proposer,
    );

    assert!(submit_response.is_ok(), "failed to submit proposal");

    let proposal_id = submit_response.unwrap().data.proposal_id;
    println!("voting on proposal: {:?}", proposal_id);
    let vote_response = gov.vote(
        MsgVote {
            proposal_id,
            voter: validator.address(),
            option: 1,
        },
        validator,
    );

    assert!(vote_response.is_ok(), "failed to vote on proposal");
}

pub fn fund_account_with_some_inj(
    bank: &Bank<InjectiveTestApp>,
    from: &SigningAccount,
    to: &SigningAccount,
) {
    bank.send(
        MsgSend {
            from_address: from.address(),
            to_address: to.address(),
            amount: vec![TubeCoin {
                amount: "1000000000000000000000".to_string(),
                denom: "inj".to_string(),
            }],
        },
        from,
    )
        .unwrap();
}

pub fn human_to_dec(raw_number: &str, decimals: &Decimals) -> String {
    let number = raw_number.replace('_', "");
    let has_decimal_fraction = number.contains(".");
    if !has_decimal_fraction {
        return format!("{}{}", number, decimals.get_right_padding_zeroes());
    }

    let separated: Vec<&str> = number.split_terminator('.').collect();
    let zeros_to_right_pad = decimals.get_decimals() - separated[1].len();
    let right_zeroes = "0".repeat(zeros_to_right_pad);
    let decimal_padded = &format!("{}{}", separated[1], right_zeroes);

    let is_below_zero = number.chars().nth(0).unwrap().to_string() == "0";
    if is_below_zero {
        // take only decimal fraction and pad with zeros
        return decimal_padded.to_string();
    }

    // take integer and decimal fraction and pad with zeros
    format!("{}{}", separated[0], decimal_padded)
}

fn remove_left_zeroes(raw_number: &str) -> String {
    let mut number = raw_number.to_string();
    while number.chars().nth(0).unwrap().to_string() == "0" {
        number.remove(0);
    }
    number
}

pub fn human_to_proto(raw_number: &str, decimals: usize) -> String {
    let number = raw_number.replace('_', "");
    let has_decimal_fraction = number.contains(".");
    let right_padding_zeroes = "0".repeat(decimals);
    if !has_decimal_fraction {
        return format!("{number}{}{}", right_padding_zeroes, Decimals::Eighteen.get_right_padding_zeroes());
    }

    let separated: Vec<&str> = number.split_terminator('.').collect();
    let zeros_to_right_pad = Decimals::Eighteen.get_decimals() - separated[1].len();
    let right_zeroes = "0".repeat(zeros_to_right_pad);

    let is_below_zero = number.chars().nth(0).unwrap().to_string() == "0";
    let decimal_padded = format!("{}{right_zeroes}", remove_left_zeroes(separated[1]));
    if is_below_zero {
        // take only decimal fraction and pad with zeros
        return decimal_padded;
    }

    // take integer pad with zeros and then the decimal fraction and also pad with zeros
    format!("{}{}{decimal_padded}", separated[0], right_padding_zeroes)
}

pub fn str_coin(human_amount: &str, denom: &str, decimals: &Decimals) -> Coin {
    let scaled_amount = human_to_dec(human_amount, decimals);
    let as_int: u128 = Uint128::from_str(scaled_amount.as_str()).unwrap().u128();
    coin(as_int, denom)
}

mod tests {
    use crate::testing::test_utils::{Decimals, human_to_dec};

    #[test]
    fn it_converts_integer_to_dec() {
        let integer = "1";
        let mut decimals = Decimals::Eighteen;
        let mut expected = "1000000000000000000";

        let actual = human_to_dec(integer, &decimals);
        assert_eq!(actual, expected, "failed to convert integer with 18 decimal to dec");

        decimals = Decimals::Six;
        expected = "1000000";

        let actual = human_to_dec(integer, &decimals);
        assert_eq!(actual, expected, "failed to convert integer with 6 decimal to dec");
    }

    
}
