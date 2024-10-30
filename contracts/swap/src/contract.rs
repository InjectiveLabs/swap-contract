use crate::{
    admin::{delete_route, save_config, set_route, update_config, withdraw_support_funds},
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg},
    queries::{estimate_swap_result, SwapQuantity},
    state::{get_all_swap_routes, get_config, read_swap_route},
    swap::{handle_atomic_order_reply, start_swap_flow},
    types::{ConfigResponse, SwapQuantityMode},
};

use cosmwasm_std::{entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::{get_contract_version, set_contract_version};
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper};

pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    Ok(Response::new().add_attribute("method", "instantiate").add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::SwapMinOutput {
            target_denom,
            min_output_quantity,
        } => start_swap_flow(deps, env, info, target_denom, SwapQuantityMode::MinOutputQuantity(min_output_quantity)),
        ExecuteMsg::SwapExactOutput {
            target_denom,
            target_output_quantity,
        } => start_swap_flow(
            deps,
            env,
            info,
            target_denom,
            SwapQuantityMode::ExactOutputQuantity(target_output_quantity),
        ),
        // Admin functions:
        ExecuteMsg::SetRoute {
            source_denom,
            target_denom,
            route,
        } => set_route(deps, &info.sender, source_denom, target_denom, route),
        ExecuteMsg::DeleteRoute { source_denom, target_denom } => delete_route(deps, &info.sender, source_denom, target_denom),
        ExecuteMsg::UpdateConfig { admin, fee_recipient } => update_config(deps, env, info.sender, admin, fee_recipient),
        ExecuteMsg::WithdrawSupportFunds { coins, target_address } => withdraw_support_funds(deps, info.sender, coins, target_address),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut<InjectiveQueryWrapper>, env: Env, msg: Reply) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg.id {
        ATOMIC_ORDER_REPLY_ID => handle_atomic_order_reply(deps, env, msg),
        _ => Err(ContractError::UnrecognizedReply(msg.id)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<InjectiveQueryWrapper>, env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::GetRoute { source_denom, target_denom } => to_json_binary(&read_swap_route(deps.storage, &source_denom, &target_denom)?),
        QueryMsg::GetOutputQuantity {
            from_quantity,
            source_denom,
            target_denom,
        } => to_json_binary(&estimate_swap_result(
            deps,
            &env,
            source_denom,
            target_denom,
            SwapQuantity::InputQuantity(from_quantity),
        )?),

        QueryMsg::GetInputQuantity {
            to_quantity,
            source_denom,
            target_denom,
        } => to_json_binary(&estimate_swap_result(
            deps,
            &env,
            source_denom,
            target_denom,
            SwapQuantity::OutputQuantity(to_quantity),
        )?),

        QueryMsg::GetAllRoutes { start_after, limit } => to_json_binary(&get_all_swap_routes(deps.storage, start_after, limit)?),

        QueryMsg::GetConfig {} => {
            let config = get_config(deps.storage)?;
            let config_response = ConfigResponse {
                config,
                contract_version: get_contract_version(deps.storage)?.version,
            };
            Ok(to_json_binary(&config_response)?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut<InjectiveQueryWrapper>, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "crates.io:swap-contract" => match contract_version.version.as_ref() {
            "1.0.1" => {
                set_contract_version(deps.storage, format!("crates.io:{CONTRACT_NAME}"), CONTRACT_VERSION)?;
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", format!("crates.io:{CONTRACT_NAME}"))
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
