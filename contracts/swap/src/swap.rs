use std::str::FromStr;

use cosmwasm_std::{
    BankMsg, Coin, DepsMut, Env, Event, MessageInfo, Reply, Response, StdResult, SubMsg, Uint128,
};

use protobuf::Message;

use crate::contract::ATOMIC_ORDER_REPLY_ID;
use injective_cosmwasm::{
    create_spot_market_order_msg, get_default_subaccount_id_for_checked_address,
    InjectiveMsgWrapper, InjectiveQuerier, InjectiveQueryWrapper, OrderType, SpotOrder,
};
use injective_math::FPDecimal;
use injective_protobuf::proto::tx;

use crate::error::ContractError;
use crate::helpers::{dec_scale_factor, round_up_to_min_tick};

use crate::queries::{estimate_single_swap_execution, estimate_swap_result, SwapQuantity};
use crate::state::{read_swap_route, CONFIG, STEP_STATE, SWAP_OPERATION_STATE, SWAP_RESULTS};
use crate::types::{
    CurrentSwapOperation, CurrentSwapStep, FPCoin, SwapEstimationAmount, SwapQuantityMode,
    SwapResults,
};

pub fn start_swap_flow(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    target_denom: String,
    swap_quantity_mode: SwapQuantityMode,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    if info.funds.len() != 1 {
        return Err(ContractError::CustomError {
            val: "Only one denom can be passed in funds".to_string(),
        });
    }
    let quantity = match swap_quantity_mode {
        SwapQuantityMode::MinOutputQuantity(q) => q,
        SwapQuantityMode::ExactOutputQuantity(q) => q,
    };

    if quantity.is_negative() || quantity.is_zero() {
        return Err(ContractError::CustomError {
            val: "Output quantity must be positive!".to_string(),
        });
    }

    let source_denom = &info.funds[0].denom;
    let route = read_swap_route(deps.storage, source_denom, &target_denom)?;
    let steps = route.steps_from(source_denom);

    let sender_address = info.sender;
    let coin_provided = &info.funds[0];

    let mut current_balance = coin_provided.to_owned().into();

    let refund_amount = if matches!(
        swap_quantity_mode,
        SwapQuantityMode::ExactOutputQuantity(..)
    ) {
        let target_output_quantity = quantity;

        let estimation = estimate_swap_result(
            deps.as_ref(),
            &env,
            source_denom.to_owned(),
            target_denom,
            SwapQuantity::OutputQuantity(target_output_quantity),
        )?;

        let querier = InjectiveQuerier::new(&deps.querier);
        let first_market_id = steps[0].to_owned();
        let first_market = querier
            .query_spot_market(&first_market_id)?
            .market
            .expect("market should be available");

        let is_input_quote = first_market.quote_denom == *source_denom;

        let required_input = if is_input_quote {
            estimation.result_quantity.int() + FPDecimal::one()
        } else {
            round_up_to_min_tick(
                estimation.result_quantity,
                first_market.min_quantity_tick_size,
            )
        };

        if required_input > coin_provided.amount.into() {
            return Err(ContractError::MaxInputAmountExceeded(required_input));
        }

        current_balance = FPCoin {
            amount: required_input,
            denom: source_denom.to_owned(),
        };

        FPDecimal::from(coin_provided.amount) - estimation.result_quantity
    } else {
        FPDecimal::zero()
    };

    let swap_operation = CurrentSwapOperation {
        sender_address,
        swap_steps: steps,
        swap_quantity_mode,
        refund: Coin::new(refund_amount.into(), source_denom.to_owned()),
    };

    SWAP_RESULTS.save(deps.storage, &Vec::new())?;
    SWAP_OPERATION_STATE.save(deps.storage, &swap_operation)?;

    execute_swap_step(deps, env, swap_operation, 0, current_balance).map_err(ContractError::Std)
}

pub fn execute_swap_step(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    swap_operation: CurrentSwapOperation,
    step_idx: u16,
    current_balance: FPCoin,
) -> StdResult<Response<InjectiveMsgWrapper>> {
    let market_id = swap_operation.swap_steps[usize::from(step_idx)].clone();
    let contract = &env.contract.address;
    let subaccount_id = get_default_subaccount_id_for_checked_address(contract);

    let estimation = estimate_single_swap_execution(
        &deps.as_ref(),
        &env,
        &market_id,
        SwapEstimationAmount::InputQuantity(current_balance.clone()),
        false,
    )?;

    let fee_recipient = &CONFIG.load(deps.storage)?.fee_recipient;

    let order = SpotOrder::new(
        estimation.worst_price,
        if estimation.is_buy_order {
            estimation.result_quantity
        } else {
            current_balance.amount
        },
        if estimation.is_buy_order {
            OrderType::BuyAtomic
        } else {
            OrderType::SellAtomic
        },
        &market_id,
        subaccount_id,
        Some(fee_recipient.to_owned()),
    );

    let order_message = SubMsg::reply_on_success(
        create_spot_market_order_msg(contract.to_owned(), order),
        ATOMIC_ORDER_REPLY_ID,
    );

    let current_step = CurrentSwapStep {
        step_idx,
        current_balance,
        step_target_denom: estimation.result_denom,
        is_buy: estimation.is_buy_order,
    };
    STEP_STATE.save(deps.storage, &current_step)?;

    let response = Response::new().add_submessage(order_message);
    Ok(response)
}

pub fn handle_atomic_order_reply(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    msg: Reply,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    let dec_scale_factor = dec_scale_factor(); // protobuf serializes Dec values with extra 10^18 factor
    let id = msg.id;
    let order_response: tx::MsgCreateSpotMarketOrderResponse = Message::parse_from_bytes(
        msg.result
            .into_result()
            .map_err(ContractError::SubMsgFailure)?
            .data
            .ok_or_else(|| ContractError::ReplyParseFailure {
                id,
                err: "Missing reply data".to_owned(),
            })?
            .as_slice(),
    )
    .map_err(|err| ContractError::ReplyParseFailure {
        id,
        err: err.to_string(),
    })?;

    let trade_data = match order_response.results.into_option() {
        Some(trade_data) => Ok(trade_data),
        None => Err(ContractError::CustomError {
            val: "No trade data in order response".to_string(),
        }),
    }?;

    // need to remove protobuf scale factor to get real values
    let average_price = FPDecimal::from_str(&trade_data.price)? / dec_scale_factor;
    let quantity = FPDecimal::from_str(&trade_data.quantity)? / dec_scale_factor;
    let fee = FPDecimal::from_str(&trade_data.fee)? / dec_scale_factor;

    let mut swap_results = SWAP_RESULTS.load(deps.storage)?;

    let current_step = STEP_STATE.load(deps.storage).map_err(ContractError::Std)?;

    let new_quantity = if current_step.is_buy {
        quantity
    } else {
        quantity * average_price - fee
    };

    let new_balance = FPCoin {
        amount: new_quantity,
        denom: current_step.step_target_denom,
    };

    let swap = SWAP_OPERATION_STATE.load(deps.storage)?;

    swap_results.push(SwapResults {
        market_id: swap.swap_steps[(current_step.step_idx) as usize].to_owned(),
        price: average_price,
        quantity,
        fee,
    });

    if current_step.step_idx < (swap.swap_steps.len() - 1) as u16 {
        SWAP_RESULTS.save(deps.storage, &swap_results)?;
        return execute_swap_step(deps, env, swap, current_step.step_idx + 1, new_balance)
            .map_err(ContractError::Std);
    }

    let min_output_quantity = match swap.swap_quantity_mode {
        SwapQuantityMode::MinOutputQuantity(q) => q,
        SwapQuantityMode::ExactOutputQuantity(q) => q,
    };

    if new_balance.amount < min_output_quantity {
        return Err(ContractError::MinOutputAmountNotReached(
            min_output_quantity,
        ));
    }

    // last step, finalize and send back funds to a caller
    let send_message = BankMsg::Send {
        to_address: swap.sender_address.to_string(),
        amount: vec![new_balance.clone().into()],
    };

    let swap_results_json = serde_json_wasm::to_string(&swap_results).unwrap();
    let swap_event = Event::new("atomic_swap_execution")
        .add_attribute("sender", swap.sender_address.to_owned())
        .add_attribute("swap_final_amount", new_balance.amount.to_string())
        .add_attribute("swap_final_denom", new_balance.denom)
        .add_attribute("swap_results", swap_results_json);

    SWAP_OPERATION_STATE.remove(deps.storage);
    STEP_STATE.remove(deps.storage);
    SWAP_RESULTS.remove(deps.storage);

    let mut response = Response::new()
        .add_message(send_message)
        .add_event(swap_event);

    if swap.refund.amount > Uint128::zero() {
        let refund_message = BankMsg::Send {
            to_address: swap.sender_address.to_string(),
            amount: vec![swap.refund],
        };
        response = response.add_message(refund_message)
    }

    Ok(response)
}
