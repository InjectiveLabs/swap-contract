use cosmwasm_std::{Addr, Deps, Env, StdError, StdResult};
use injective_cosmwasm::{InjectiveQuerier, InjectiveQueryWrapper, MarketId, OrderSide, PriceLevel, SpotMarket};
use injective_math::FPDecimal;

use crate::exact_math::{multiply_to_raw, ratio_to_tick, select_liquidity_levels, weighted_average_to_tick, LiquidityTarget, RoundingDirection};
use crate::helpers::round_up_to_min_tick;
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
                return Err(StdError::msg("source_quantity must be positive"));
            }
        }
        SwapQuantity::OutputQuantity(quantity) => {
            if quantity.is_zero() || quantity.is_negative() {
                return Err(StdError::msg("target_quantity must be positive"));
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
                SwapQuantity::InputQuantity(_) => SwapEstimationAmount::InputQuantity(current_swap.clone()),
                SwapQuantity::OutputQuantity(_) => SwapEstimationAmount::ReceiveQuantity(current_swap.clone()),
            },
            true,
        )?;

        current_swap.amount = swap_estimate.result_quantity;
        current_swap.denom = swap_estimate.result_denom;

        let step_fee = swap_estimate.fee_estimate.expect("fee estimate should be available");

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

    let market = querier.query_spot_market(market_id)?.market.expect("market should be available");

    let has_invalid_denom = balance_in.denom != market.quote_denom && balance_in.denom != market.base_denom;
    if has_invalid_denom {
        return Err(StdError::msg("Invalid swap denom - neither base nor quote"));
    }

    let config = CONFIG.load(deps.storage)?;
    let is_self_relayer = config.fee_recipient == env.contract.address;

    let fee_multiplier = querier.query_market_atomic_execution_fee_multiplier(market_id)?.multiplier;

    let fee_percent = market.taker_fee_rate * fee_multiplier * (FPDecimal::ONE - get_effective_fee_discount_rate(&market, is_self_relayer));

    let is_estimating_from_target = matches!(swap_estimation_amount, SwapEstimationAmount::ReceiveQuantity(_));

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
        estimate_execution_sell(&querier, &market, swap_estimation_amount, fee_percent)
    }
}

fn estimate_execution_buy_from_source(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    contract_address: &Addr,
    market: &SpotMarket,
    input_quote_quantity: FPDecimal,
    fee_percent: FPDecimal,
    is_simulation: bool,
) -> StdResult<StepExecutionEstimate> {
    let raw_tick = FPDecimal::from(primitive_types::U256::one());
    let fee_factor = FPDecimal::ONE + fee_percent;
    let available_swap_quote_funds = ratio_to_tick(input_quote_quantity, fee_factor, raw_tick, RoundingDirection::Down)?;

    let orders = querier.query_spot_market_orderbook(&market.market_id, OrderSide::Sell, None, Some(available_swap_quote_funds))?;
    let top_orders = select_liquidity_levels(
        &orders.sells_price_level,
        available_swap_quote_funds,
        LiquidityTarget::Notional,
        market.min_quantity_tick_size,
    )?;

    // lets overestimate amount for buys means rounding average price up -> higher buy price -> worse
    let average_price = weighted_average_to_tick(&top_orders, market.min_price_tick_size, RoundingDirection::Up)?;
    let worst_price = get_worst_price_from_orders(&top_orders);

    let result_quantity = ratio_to_tick(
        available_swap_quote_funds,
        average_price,
        market.min_quantity_tick_size,
        RoundingDirection::Down,
    )?;
    let fee_estimate = input_quote_quantity - available_swap_quote_funds;

    // check if user funds + contract funds are enough to create order
    let worst_price_notional = multiply_to_raw(worst_price, result_quantity, RoundingDirection::Up)?;
    let required_funds = multiply_to_raw(worst_price_notional, fee_factor, RoundingDirection::Up)?;
    let funds_in_contract = deps
        .querier
        .query_balance(contract_address, &market.quote_denom)
        .expect("query own balance should not fail")
        .amount
        .into();

    let funds_for_margin = match is_simulation {
        false => funds_in_contract, // in execution mode funds_in_contract already contain user funds so we don't want to count them double
        true => funds_in_contract + input_quote_quantity,
    };

    if required_funds > funds_for_margin {
        return Err(StdError::msg(format!(
            "Swap amount too high, required funds: {required_funds}, available funds: {funds_for_margin}",
        )));
    }

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity,
        result_denom: market.base_denom.to_string(),
        is_buy_order: true,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
}

fn estimate_execution_buy_from_target(
    deps: &Deps<InjectiveQueryWrapper>,
    querier: &InjectiveQuerier,
    contract_address: &Addr,
    market: &SpotMarket,
    target_base_output_quantity: FPDecimal,
    fee_percent: FPDecimal,
    is_simulation: bool,
) -> StdResult<StepExecutionEstimate> {
    let rounded_target_base_output_quantity = round_up_to_min_tick(target_base_output_quantity, market.min_quantity_tick_size);

    let orders = querier.query_spot_market_orderbook(&market.market_id, OrderSide::Sell, Some(rounded_target_base_output_quantity), None)?;
    let top_orders = select_liquidity_levels(
        &orders.sells_price_level,
        rounded_target_base_output_quantity,
        LiquidityTarget::Quantity,
        market.min_quantity_tick_size,
    )?;

    // lets overestimate amount for buys means rounding average price up -> higher buy price -> worse
    let average_price = weighted_average_to_tick(&top_orders, market.min_price_tick_size, RoundingDirection::Up)?;
    let worst_price = get_worst_price_from_orders(&top_orders);

    let expected_exchange_quote_quantity = multiply_to_raw(rounded_target_base_output_quantity, average_price, RoundingDirection::Up)?;
    let required_input_quote_quantity = multiply_to_raw(expected_exchange_quote_quantity, FPDecimal::ONE + fee_percent, RoundingDirection::Up)?;
    let fee_estimate = required_input_quote_quantity - expected_exchange_quote_quantity;

    // check if user funds + contract funds are enough to create order
    let worst_price_notional = multiply_to_raw(worst_price, rounded_target_base_output_quantity, RoundingDirection::Up)?;
    let required_funds = multiply_to_raw(worst_price_notional, FPDecimal::ONE + fee_percent, RoundingDirection::Up)?;

    let funds_in_contract = deps
        .querier
        .query_balance(contract_address, &market.quote_denom)
        .expect("query own balance should not fail")
        .amount
        .into();

    let funds_for_margin = match is_simulation {
        false => funds_in_contract, // in execution mode funds_in_contract already contain user funds so we don't want to count them double
        true => funds_in_contract + required_input_quote_quantity,
    };

    if required_funds > funds_for_margin {
        return Err(StdError::msg(format!(
            "Swap amount too high, required funds: {required_funds}, available funds: {funds_for_margin}",
        )));
    }

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity: required_input_quote_quantity,
        result_denom: market.quote_denom.to_string(),
        is_buy_order: true,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
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

    let is_estimating_from_target = matches!(swap_estimation_amount, SwapEstimationAmount::ReceiveQuantity(_));

    if is_estimating_from_target {
        estimate_execution_buy_from_target(deps, querier, contract_address, market, amount_coin.amount, fee_percent, is_simulation)
    } else {
        estimate_execution_buy_from_source(deps, querier, contract_address, market, amount_coin.amount, fee_percent, is_simulation)
    }
}

fn estimate_execution_sell_from_source(
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    input_base_quantity: FPDecimal,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    let orders = querier.query_spot_market_orderbook(&market.market_id, OrderSide::Buy, Some(input_base_quantity), None)?;

    let top_orders = select_liquidity_levels(
        &orders.buys_price_level,
        input_base_quantity,
        LiquidityTarget::Quantity,
        market.min_quantity_tick_size,
    )?;

    // lets overestimate amount for sells means rounding average price down -> lower sell price -> worse
    let average_price = weighted_average_to_tick(&top_orders, market.min_price_tick_size, RoundingDirection::Down)?;
    let worst_price = get_worst_price_from_orders(&top_orders);

    let expected_exchange_quantity = multiply_to_raw(input_base_quantity, average_price, RoundingDirection::Down)?;
    let expected_quantity = multiply_to_raw(expected_exchange_quantity, FPDecimal::ONE - fee_percent, RoundingDirection::Down)?;
    let fee_estimate = expected_exchange_quantity - expected_quantity;

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity: expected_quantity,
        result_denom: market.quote_denom.to_string(),
        is_buy_order: false,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: fee_estimate,
        }),
    })
}

fn estimate_execution_sell_from_target(
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    target_quote_output_quantity: FPDecimal,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    let required_swap_quantity_in_quote = ratio_to_tick(
        target_quote_output_quantity,
        FPDecimal::ONE - fee_percent,
        FPDecimal::from(primitive_types::U256::one()),
        RoundingDirection::Up,
    )?;
    let required_fee = required_swap_quantity_in_quote - target_quote_output_quantity;

    let orders = querier.query_spot_market_orderbook(&market.market_id, OrderSide::Buy, None, Some(required_swap_quantity_in_quote))?;
    let top_orders = select_liquidity_levels(
        &orders.buys_price_level,
        required_swap_quantity_in_quote,
        LiquidityTarget::Notional,
        market.min_quantity_tick_size,
    )?;

    // lets overestimate amount for sells means rounding average price down -> lower sell price -> worse
    let average_price = weighted_average_to_tick(&top_orders, market.min_price_tick_size, RoundingDirection::Down)?;
    let worst_price = get_worst_price_from_orders(&top_orders);

    let required_swap_input_quantity_in_base = ratio_to_tick(
        required_swap_quantity_in_quote,
        average_price,
        market.min_quantity_tick_size,
        RoundingDirection::Up,
    )?;

    Ok(StepExecutionEstimate {
        worst_price,
        result_quantity: required_swap_input_quantity_in_base,
        result_denom: market.base_denom.to_string(),
        is_buy_order: false,
        fee_estimate: Some(FPCoin {
            denom: market.quote_denom.clone(),
            amount: required_fee,
        }),
    })
}

fn estimate_execution_sell(
    querier: &InjectiveQuerier,
    market: &SpotMarket,
    swap_estimation_amount: SwapEstimationAmount,
    fee_percent: FPDecimal,
) -> StdResult<StepExecutionEstimate> {
    let amount_coin = match swap_estimation_amount.to_owned() {
        SwapEstimationAmount::InputQuantity(fp) => fp,
        SwapEstimationAmount::ReceiveQuantity(fp) => fp,
    };

    let is_estimating_from_target = matches!(swap_estimation_amount, SwapEstimationAmount::ReceiveQuantity(_));

    if is_estimating_from_target {
        estimate_execution_sell_from_target(querier, market, amount_coin.amount, fee_percent)
    } else {
        estimate_execution_sell_from_source(querier, market, amount_coin.amount, fee_percent)
    }
}

fn get_worst_price_from_orders(levels: &[PriceLevel]) -> FPDecimal {
    levels.last().unwrap().p // assume there's at least one element
}

fn get_effective_fee_discount_rate(market: &SpotMarket, is_self_relayer: bool) -> FPDecimal {
    if !is_self_relayer {
        FPDecimal::ZERO
    } else {
        market.relayer_fee_share_rate
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::test_utils::create_price_level;

    use super::*;

    #[test]
    fn test_average_price_simple() {
        let levels = vec![create_price_level(1, 200), create_price_level(2, 200), create_price_level(3, 200)];

        let avg = weighted_average_to_tick(&levels, FPDecimal::must_from_str("0.01"), RoundingDirection::Down).unwrap();
        assert_eq!(avg, FPDecimal::from(2u128));
    }

    #[test]
    fn test_average_price_simple_round_down() {
        let levels = vec![create_price_level(1, 300), create_price_level(2, 200), create_price_level(3, 100)];

        let avg = weighted_average_to_tick(&levels, FPDecimal::must_from_str("0.01"), RoundingDirection::Down).unwrap();
        assert_eq!(avg, FPDecimal::must_from_str("1.66")); //we round down
    }

    #[test]
    fn test_average_price_simple_round_up() {
        let levels = vec![create_price_level(1, 300), create_price_level(2, 200), create_price_level(3, 100)];

        let avg = weighted_average_to_tick(&levels, FPDecimal::must_from_str("0.01"), RoundingDirection::Up).unwrap();
        assert_eq!(avg, FPDecimal::must_from_str("1.67")); //we round up
    }

    #[test]
    fn test_worst_price() {
        let levels = vec![create_price_level(1, 100), create_price_level(2, 200), create_price_level(3, 300)];

        let worst = get_worst_price_from_orders(&levels);
        assert_eq!(worst, FPDecimal::from(3u128));
    }

    #[test]
    fn test_find_minimum_orders_not_enough_liquidity() {
        let levels = vec![create_price_level(1, 100), create_price_level(2, 200)];

        let result = select_liquidity_levels(
            &levels,
            FPDecimal::from(1000u128),
            LiquidityTarget::Quantity,
            FPDecimal::must_from_str("0.01"),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().as_ref().to_string(), "Not enough liquidity to fulfill order");
    }

    #[test]
    fn test_find_minimum_orders_with_gaps() {
        let levels = vec![create_price_level(1, 100), create_price_level(3, 300), create_price_level(5, 500)];

        let result = select_liquidity_levels(
            &levels,
            FPDecimal::from(800u128),
            LiquidityTarget::Quantity,
            FPDecimal::must_from_str("0.01"),
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
        let levels = vec![create_price_level(1, 100), create_price_level(3, 300), create_price_level(5, 500)];

        let result = select_liquidity_levels(
            &levels,
            FPDecimal::from(450u128),
            LiquidityTarget::Quantity,
            FPDecimal::must_from_str("0.01"),
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
        let buy_levels = vec![create_price_level(5, 500), create_price_level(3, 300), create_price_level(1, 100)];

        let result = select_liquidity_levels(
            &buy_levels,
            FPDecimal::from(3450u128),
            LiquidityTarget::Notional,
            FPDecimal::must_from_str("0.01"),
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
