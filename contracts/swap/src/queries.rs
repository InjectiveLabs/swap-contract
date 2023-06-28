use cosmwasm_std::{Addr, Deps, Env, StdError, StdResult};
use injective_cosmwasm::{
    InjectiveQuerier, InjectiveQueryWrapper, MarketId, OrderSide, PriceLevel, SpotMarket,
};
use injective_math::utils::round_to_min_tick;
use injective_math::FPDecimal;

use crate::helpers::{counter_denom, round_up_to_min_tick};
use crate::state::{read_swap_route, CONFIG};
use crate::types::{FPCoin, StepExecutionEstimate, SwapEstimationAmount, SwapEstimationResult};

pub enum SwapQuantity {
    InputQuantity(FPDecimal),
    OutputQuantity(FPDecimal),
}

pub fn estimate_swap_result(
    deps: Deps<InjectiveQueryWrapper>,
    env: &Env,
    source_denom: String,
    target_denom: String,
    swap_quantity: SwapQuantity,
) -> StdResult<SwapEstimationResult> {
    match swap_quantity {
        SwapQuantity::InputQuantity(quantity) => {
            if quantity.is_zero() || quantity.is_negative() {
                return Err(StdError::generic_err("source_quantity must be positive"));
            }
        }
        SwapQuantity::OutputQuantity(quantity) => {
            if quantity.is_zero() || quantity.is_negative() {
                return Err(StdError::generic_err("target_quantity must be positive"));
            }
        }
    }

    let route = read_swap_route(deps.storage, &source_denom, &target_denom)?;

    let (steps, mut current_swap) = match swap_quantity {
        SwapQuantity::InputQuantity(quantity) => (
            route.steps_from(&source_denom),
            FPCoin {
                amount: quantity,
                denom: source_denom.clone(),
            },
        ),
        SwapQuantity::OutputQuantity(quantity) => {
            let mut steps = route.steps_from(&source_denom);
            steps.reverse();
            (
                steps,
                FPCoin {
                    amount: quantity,
                    denom: target_denom,
                },
            )
        }
    };

    let mut fees: Vec<FPCoin> = vec![];

    for step in steps {
        let swap_estimate = estimate_single_swap_execution(
            &deps,
            env,
            &step,
            match swap_quantity {
                SwapQuantity::InputQuantity(_) => {
                    SwapEstimationAmount::InputQuantity(current_swap.clone())
                }
                SwapQuantity::OutputQuantity(_) => {
                    SwapEstimationAmount::ReceiveQuantity(current_swap.clone())
                }
            },
            true,
        )?;

        let new_amount = swap_estimate.result_quantity;

        current_swap.amount = new_amount;
        current_swap.denom = swap_estimate.result_denom;

        let step_fee = swap_estimate
            .fee_estimate
            .expect("fee estimate should be available");

        fees.push(step_fee);
    }

    Ok(SwapEstimationResult {
        expected_fees: fees,
        result_quantity: current_swap.amount,
    })
}

pub fn estimate_single_swap_execution(
    deps: &Deps<InjectiveQueryWrapper>,
    env: &Env,
    market_id: &MarketId,
    swap_estimation_amount: SwapEstimationAmount,
    is_simulation: bool,
) -> StdResult<StepExecutionEstimate> {
    let querier = InjectiveQuerier::new(&deps.querier);

    let balance_in = match swap_estimation_amount.to_owned() {
        SwapEstimationAmount::InputQuantity(fp) => fp,
        SwapEstimationAmount::ReceiveQuantity(fp) => fp,
    };

    let market = querier
        .query_spot_market(market_id)?
        .market
        .expect("market should be available");

    let has_invalid_denom =
        balance_in.denom != market.quote_denom && balance_in.denom != market.base_denom;
    if has_invalid_denom {
        return Err(StdError::generic_err(
            "Invalid swap denom - neither base nor quote",
        ));
    }

    let config = CONFIG.load(deps.storage)?;
    let is_self_relayer = config.fee_recipient == env.contract.address;

    let fee_multiplier = querier
        .query_market_atomic_execution_fee_multiplier(market_id)?
        .multiplier;
    let fee_percent = market.taker_fee_rate
        * fee_multiplier
        * (FPDecimal::one() - get_effective_fee_discount_rate(&market, is_self_relayer));

    let is_estimating_from_target = matches!(
        swap_estimation_amount,
        SwapEstimationAmount::ReceiveQuantity(_)
    );

    let is_buy = if is_estimating_from_target {
        balance_in.denom == market.base_denom
    } else {
        balance_in.denom != market.base_denom
    };

    if is_buy {
        estimate_execution_buy(
            deps,
            &querier,
            &env.contract.address,
            &market,
            swap_estimation_amount,
            fee_percent,
            is_simulation,
        )
    } else {
        estimate_execution_sell(deps, &querier, &market, swap_estimation_amount, fee_percent)
    }
}

fn estimate_execution_buy(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    contract_address: &Addr,
    market: &SpotMarket,
    swap_estimation_amount: SwapEstimationAmount,
    fee_percent: FPDecimal,
    is_simulation: bool,
) -> StdResult<StepExecutionEstimate> {
    let amount_coin = match swap_estimation_amount.to_owned() {
        SwapEstimationAmount::InputQuantity(fp) => fp,
        SwapEstimationAmount::ReceiveQuantity(fp) => fp,
    };

    let is_estimating_from_source = matches!(
        swap_estimation_amount,
        SwapEstimationAmount::InputQuantity(_)
    );

    let available_funds = if is_estimating_from_source {
        // keep reserve for fee
        Some(amount_coin.amount / (FPDecimal::one() + fee_percent))
    } else {
        // unknown when estimating from target
        None
    };

    let top_orders = if is_estimating_from_source {
        let orders = querier.query_spot_market_orderbook(
            &market.market_id,
            OrderSide::Sell,
            None,
            available_funds,
        )?;
        get_minimum_liquidity_levels(
            deps,
            &orders.sells_price_level,
            available_funds.unwrap(),
            |l| l.q * l.p,
        )?
    } else {
        let orders = querier.query_spot_market_orderbook(
            &market.market_id,
            OrderSide::Sell,
            Some(amount_coin.amount),
            None,
        )?;
        get_minimum_liquidity_levels(deps, &orders.sells_price_level, amount_coin.amount, |l| l.q)?
    };

    let worst_price = get_worst_price_from_orders(&top_orders);
    let average_price = get_average_price_from_orders(&top_orders);

    let (expected_quantity, result_quantity, fee_estimate) = if is_estimating_from_source {
        let expected_quantity = available_funds.unwrap() / average_price;
        let result_quantity = round_to_min_tick(expected_quantity, market.min_quantity_tick_size);
        let fee_estimate = amount_coin.amount - available_funds.unwrap();

        (expected_quantity, result_quantity, fee_estimate)
    } else {
        let expected_exchange_quantity = amount_coin.amount * average_price;
        let fee_estimate = expected_exchange_quantity * fee_percent;

        let result_quantity = round_up_to_min_tick(
            expected_exchange_quantity + fee_estimate,
            market.min_price_tick_size,
        );

        (expected_exchange_quantity, result_quantity, fee_estimate)
    };

    // check if user funds + contract funds are enough to create order
    let required_funds = worst_price * expected_quantity * (FPDecimal::one() + fee_percent);
    let funds_in_contract: FPDecimal = deps
        .querier
        .query_balance(contract_address, &market.quote_denom)
        .expect("query own balance should not fail")
        .amount
        .into();

    let funds_for_margin = match is_simulation {
        false => funds_in_contract, // / in execution mode funds_in_contract already contain user funds so we don't want to count them double
        true => funds_in_contract + available_funds.unwrap_or_default(),
    };

    if required_funds > funds_for_margin {
        return Err(StdError::generic_err(format!(
            "Swap amount too high, required funds: {required_funds}, available funds: {funds_for_margin}",
        )));
    }

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity,
        result_denom: counter_denom(market, &amount_coin.denom)?.to_string(),
        is_buy_order: true,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
}

fn estimate_execution_sell_from_source(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    amount_coin: FPCoin,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    let orders = querier.query_spot_market_orderbook(
        &market.market_id,
        OrderSide::Buy,
        Some(amount_coin.amount),
        None,
    )?;

    let top_orders =
        get_minimum_liquidity_levels(deps, &orders.buys_price_level, amount_coin.amount, |l| l.q)?;

    let average_price = get_average_price_from_orders(&top_orders);
    let expected_exchange_quantity = amount_coin.amount * average_price;
    let fee_estimate = expected_exchange_quantity * fee_percent;

    let expected_quantity = expected_exchange_quantity - fee_estimate;

    let worst_price = get_worst_price_from_orders(&top_orders);

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity: expected_quantity,
        result_denom: counter_denom(market, &amount_coin.denom)?.to_string(),
        is_buy_order: false,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
}

fn estimate_execution_sell_from_target(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    amount_coin: FPCoin,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    // let available_funds = amount_coin.amount;
    let available_funds = round_to_min_tick(
        amount_coin.amount + amount_coin.amount * fee_percent,
        market.min_quantity_tick_size,
    );
    let fee_estimate = amount_coin.amount - available_funds;

    let orders = querier.query_spot_market_orderbook(
        &market.market_id,
        OrderSide::Buy,
        None,
        Some(available_funds),
    )?;
    let top_orders =
        get_minimum_liquidity_levels(deps, &orders.buys_price_level, available_funds, |l| {
            l.q * l.p
        })?;

    let average_price = get_average_price_from_orders(&top_orders);
    let expected_input_quantity = amount_coin.amount / average_price;
    let worst_price = get_worst_price_from_orders(&top_orders);

    let fee_estimate = expected_input_quantity * fee_percent;
    let expected_input_quantity_with_fee = expected_input_quantity + fee_estimate;

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity: round_up_to_min_tick(
            expected_input_quantity_with_fee,
            market.min_price_tick_size,
        ),
        result_denom: counter_denom(market, &amount_coin.denom)?.to_string(),
        is_buy_order: false,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
}

fn estimate_execution_sell(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    swap_estimation_amount: SwapEstimationAmount,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    let amount_coin = match swap_estimation_amount.to_owned() {
        SwapEstimationAmount::InputQuantity(fp) => fp,
        SwapEstimationAmount::ReceiveQuantity(fp) => fp,
    };

    let is_estimating_from_target = matches!(
        swap_estimation_amount,
        SwapEstimationAmount::ReceiveQuantity(_)
    );

    if is_estimating_from_target {
        estimate_execution_sell_from_target(deps, querier, market, amount_coin, fee_percent)
    } else {
        estimate_execution_sell_from_source(deps, querier, market, amount_coin, fee_percent)
    }
}

pub fn get_minimum_liquidity_levels(
    _deps: &Deps<InjectiveQueryWrapper>,
    levels: &Vec<PriceLevel>,
    total: FPDecimal,
    calc: fn(&PriceLevel) -> FPDecimal,
) -> StdResult<Vec<PriceLevel>> {
    let mut sum = FPDecimal::zero();
    let mut orders: Vec<PriceLevel> = Vec::new();

    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("total: {total}");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");
    println!("---------------");

    for level in levels {
        let value = calc(level);

        let order_to_add = if sum + value > total {
            let excess = value + sum - total;

            PriceLevel {
                p: level.p,
                q: ((value - excess) / value) * level.q, // we only take a part of this price level
            }
        } else {
            level.clone() // take fully
        };

        sum += value;
        orders.push(order_to_add);

        if sum >= total {
            break;
        }
    }

    if sum < total {
        return Err(StdError::generic_err(
            "Not enough liquidity to fulfill order",
        ));
    }

    Ok(orders)
}

fn get_average_price_from_orders(levels: &[PriceLevel]) -> FPDecimal {
    let (total_quantity, total_notional) = levels
        .iter()
        .fold((FPDecimal::zero(), FPDecimal::zero()), |acc, pl| {
            (acc.0 + pl.q, acc.1 + pl.p * pl.q)
        });
    total_notional / total_quantity
}

fn get_worst_price_from_orders(levels: &[PriceLevel]) -> FPDecimal {
    levels.last().unwrap().p // assume there's at least one element
}

fn get_effective_fee_discount_rate(market: &SpotMarket, is_self_relayer: bool) -> FPDecimal {
    if !is_self_relayer {
        FPDecimal::zero()
    } else {
        market.relayer_fee_share_rate
    }
}

#[cfg(test)]
mod tests {
    use injective_cosmwasm::inj_mock_deps;

    use crate::testing::test_utils::create_price_level;

    use super::*;

    #[test]
    fn test_average_price_simple() {
        let levels = vec![
            create_price_level(1, 200),
            create_price_level(2, 200),
            create_price_level(3, 200),
        ];

        let avg = get_average_price_from_orders(&levels);
        assert_eq!(avg, FPDecimal::from(2u128));
    }

    #[test]
    fn test_average_price_simple_2() {
        let levels = vec![
            create_price_level(1, 300),
            create_price_level(2, 200),
            create_price_level(3, 100),
        ];

        let avg = get_average_price_from_orders(&levels);
        assert_eq!(avg, FPDecimal::from(1000u128) / FPDecimal::from(600u128));
    }

    #[test]
    fn test_worst_price() {
        let levels = vec![
            create_price_level(1, 100),
            create_price_level(2, 200),
            create_price_level(3, 300),
        ];

        let worst = get_worst_price_from_orders(&levels);
        assert_eq!(worst, FPDecimal::from(3u128));
    }

    #[test]
    fn test_find_minimum_orders_not_enough_liquidity() {
        let levels = vec![create_price_level(1, 100), create_price_level(2, 200)];

        let result = get_minimum_liquidity_levels(
            &inj_mock_deps(|_| {}).as_ref(),
            &levels,
            FPDecimal::from(1000u128),
            |l| l.q,
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            StdError::generic_err("Not enough liquidity to fulfill order")
        );
    }

    #[test]
    fn test_find_minimum_orders_with_gaps() {
        let levels = vec![
            create_price_level(1, 100),
            create_price_level(3, 300),
            create_price_level(5, 500),
        ];

        let result = get_minimum_liquidity_levels(
            &inj_mock_deps(|_| {}).as_ref(),
            &levels,
            FPDecimal::from(800u128),
            |l| l.q,
        );
        assert!(result.is_ok());
        let min_orders = result.unwrap();
        assert_eq!(min_orders.len(), 3);
        assert_eq!(min_orders[0].p, FPDecimal::from(1u128));
        assert_eq!(min_orders[1].p, FPDecimal::from(3u128));
        assert_eq!(min_orders[2].p, FPDecimal::from(5u128));
    }

    #[test]
    fn test_find_minimum_buy_orders_not_consuming_fully() {
        let levels = vec![
            create_price_level(1, 100),
            create_price_level(3, 300),
            create_price_level(5, 500),
        ];

        let result = get_minimum_liquidity_levels(
            &inj_mock_deps(|_| {}).as_ref(),
            &levels,
            FPDecimal::from(450u128),
            |l| l.q,
        );
        assert!(result.is_ok());
        let min_orders = result.unwrap();
        assert_eq!(min_orders.len(), 3);
        assert_eq!(min_orders[0].p, FPDecimal::from(1u128));
        assert_eq!(min_orders[0].q, FPDecimal::from(100u128));
        assert_eq!(min_orders[1].p, FPDecimal::from(3u128));
        assert_eq!(min_orders[1].q, FPDecimal::from(300u128));
        assert_eq!(min_orders[2].p, FPDecimal::from(5u128));
        assert_eq!(min_orders[2].q, FPDecimal::from(50u128));
    }

    #[test]
    fn test_find_minimum_sell_orders_not_consuming_fully() {
        let buy_levels = vec![
            create_price_level(5, 500),
            create_price_level(3, 300),
            create_price_level(1, 100),
        ];

        let result = get_minimum_liquidity_levels(
            &inj_mock_deps(|_| {}).as_ref(),
            &buy_levels,
            FPDecimal::from(3450u128),
            |l| l.q * l.p,
        );
        assert!(result.is_ok());
        let min_orders = result.unwrap();
        assert_eq!(min_orders.len(), 3);
        assert_eq!(min_orders[0].p, FPDecimal::from(5u128));
        assert_eq!(min_orders[0].q, FPDecimal::from(500u128));
        assert_eq!(min_orders[1].p, FPDecimal::from(3u128));
        assert_eq!(min_orders[1].q, FPDecimal::from(300u128));
        assert_eq!(min_orders[2].p, FPDecimal::from(1u128));
        assert_eq!(min_orders[2].q, FPDecimal::from(50u128));
    }
}
