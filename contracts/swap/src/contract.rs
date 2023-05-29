use std::collections::HashSet;
use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdResult, SubMsg,
};
use cw2::set_contract_version;
use protobuf::Message;

use injective_cosmwasm::{
    create_spot_market_order_msg, get_default_subaccount_id_for_checked_address,
    InjectiveMsgWrapper, InjectiveQueryWrapper, MarketId, OrderType, SpotOrder,
};
use injective_math::FPDecimal;
use injective_protobuf::proto::tx;
use crate::admin::{delete_route, save_config, set_route, update_config, withdraw_support_funds};

use crate::error::ContractError;
use crate::helpers::dec_scale_factor;
use crate::msg::{ExecuteMsg, FeeRecipient, InstantiateMsg, QueryMsg};
use crate::queries::{estimate_single_swap_execution, estimate_swap_result};
use crate::state::{
    read_swap_route, remove_swap_route, store_swap_route, CONFIG, STEP_STATE, SWAP_OPERATION_STATE,
};
use crate::swap::{execute_swap_step, handle_atomic_order_reply, start_swap_flow};
use crate::types::{Config, CurrentSwapOperation, CurrentSwapStep, FPCoin, SwapRoute};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:atomic-order-example";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ATOMIC_ORDER_REPLY_ID: u64 = 1u64;
pub const DEPOSIT_REPLY_ID: u64 = 2u64;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    save_config(deps, env, msg.admin, msg.fee_recipient)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Swap {
            target_denom,
            min_quantity,
        } => start_swap_flow(deps, env, info, target_denom, min_quantity),
        // Admin functions:
        ExecuteMsg::SetRoute {
            source_denom,
            target_denom,
            route,
        } => set_route(deps, &info.sender, source_denom, target_denom, route),
        ExecuteMsg::DeleteRoute {
            source_denom,
            target_denom,
        } => delete_route(deps, &info.sender, source_denom, target_denom),
        ExecuteMsg::UpdateConfig {
            admin,
            fee_recipient,
        } => update_config(deps, env, info.sender, admin, fee_recipient),
        ExecuteMsg::WithdrawSupportFunds {
            coins,
            target_address,
        } => withdraw_support_funds(deps, info.sender, coins, target_address),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    msg: Reply,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg.id {
        ATOMIC_ORDER_REPLY_ID => handle_atomic_order_reply(deps, env, msg),
        _ => Err(ContractError::UnrecognizedReply(msg.id)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<InjectiveQueryWrapper>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetRoute {
            source_denom,
            target_denom,
        } => Ok(to_binary(&read_swap_route(
            deps.storage,
            &source_denom,
            &target_denom,
        )?)?),
        QueryMsg::GetExecutionQuantity {
            from_quantity,
            source_denom,
            to_denom,
        } => {
            let target_quantity =
                estimate_swap_result(deps, env, source_denom, from_quantity, to_denom)?;
            Ok(to_binary(&target_quantity)?)
        }
    }
}
