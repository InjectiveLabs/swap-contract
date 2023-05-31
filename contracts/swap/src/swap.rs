use std::str::FromStr;

use cosmwasm_std::{BankMsg, DepsMut, Env, MessageInfo, Reply, Response, StdResult, SubMsg};

use protobuf::Message;

use crate::contract::ATOMIC_ORDER_REPLY_ID;
use injective_cosmwasm::{
    create_spot_market_order_msg, get_default_subaccount_id_for_checked_address,
    InjectiveMsgWrapper, InjectiveQueryWrapper, OrderType, SpotOrder,
};
use injective_math::FPDecimal;
use injective_protobuf::proto::tx;

use crate::error::ContractError;
use crate::helpers::dec_scale_factor;

use crate::queries::estimate_single_swap_execution;
use crate::state::{read_swap_route, CONFIG, STEP_STATE, SWAP_OPERATION_STATE};
use crate::types::{CurrentSwapOperation, CurrentSwapStep, FPCoin};

pub fn start_swap_flow(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    target_denom: String,
    min_target_quantity: FPDecimal,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    if info.funds.len() != 1 {
        return Err(ContractError::CustomError {
            val: "Only one denom can be passed in funds".to_string(),
        });
    }
    if min_target_quantity.is_negative() || min_target_quantity.is_zero() {
        return Err(ContractError::CustomError {
            val: "Min target quantity must be positive!".to_string(),
        });
    }

    let coin_provided = info.funds[0].clone();
    let source_denom = coin_provided.denom.clone();
    let route = read_swap_route(deps.storage, &source_denom, &target_denom)?;
    let steps = route.steps_from(&source_denom);

    let current_balance: FPCoin = coin_provided.into();
    let swap_operation = CurrentSwapOperation {
        sender_address: info.sender,
        swap_steps: steps,
        min_target_quantity,
    };
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
        current_balance.clone(),
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

    let response: Response<InjectiveMsgWrapper> = Response::new().add_submessage(order_message);
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

    // unwrap results into trade_data
    let trade_data = match order_response.results.into_option() {
        Some(trade_data) => Ok(trade_data),
        None => Err(ContractError::CustomError {
            val: "No trade data in order response".to_string(),
        }),
    }?;
    let quantity = FPDecimal::from_str(&trade_data.quantity)? / dec_scale_factor; // need to remove protobuf scale factor to get real values
    let avg_price = FPDecimal::from_str(&trade_data.price)? / dec_scale_factor;
    let fee = FPDecimal::from_str(&trade_data.fee)? / dec_scale_factor;
    deps.api.debug(&format!(
        "Quantity: {quantity}, price {avg_price}, fee {fee}"
    ));

    let current_step = STEP_STATE.load(deps.storage).map_err(ContractError::Std)?;
    let new_quantity = if current_step.is_buy {
        quantity
    } else {
        quantity * avg_price - fee
    };

    let new_balance = FPCoin {
        amount: new_quantity,
        denom: current_step.step_target_denom,
    };

    deps.api.debug(&format!("New balance: {new_balance:?}"));

    let swap = SWAP_OPERATION_STATE.load(deps.storage)?;
    if current_step.step_idx < (swap.swap_steps.len() - 1) as u16 {
        execute_swap_step(deps, env, swap, current_step.step_idx + 1, new_balance)
            .map_err(ContractError::Std)
    } else {
        // last step, finalize and send back funds to a caller
        if new_balance.amount < swap.min_target_quantity {
            return Err(ContractError::MinExpectedSwapAmountNotReached(
                swap.min_target_quantity,
            ));
        }
        let send_message = BankMsg::Send {
            to_address: swap.sender_address.to_string(),
            amount: vec![new_balance.clone().into()],
        };
        deps.api.debug(&format!("Send message: {send_message:?}"));
        SWAP_OPERATION_STATE.remove(deps.storage);
        STEP_STATE.remove(deps.storage);
        let response = Response::new()
            .add_message(send_message)
            .add_attribute("swap_final_amount", new_balance.amount.to_string())
            .add_attribute("swap_final_denom", new_balance.denom);

        Ok(response)
    }
}
