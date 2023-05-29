use std::collections::HashSet;
use cosmwasm_std::{Addr, BankMsg, Coin, Deps, DepsMut, Env, Response, StdResult};
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper, MarketId};
use crate::ContractError;
use crate::msg::FeeRecipient;
use crate::state::{CONFIG, remove_swap_route, store_swap_route};
use crate::types::{Config, SwapRoute};

pub fn save_config(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    admin: Addr,
    fee_recipient: FeeRecipient,
) -> StdResult<()> {
    let fee_recipient = match fee_recipient {
        FeeRecipient::Address(addr) => addr,
        FeeRecipient::SwapContract => env.contract.address,
    };
    let config = Config {
        fee_recipient,
        admin,
    };
    CONFIG.save(deps.storage, &config)
}

pub fn verify_sender_is_admin(
    deps: Deps<InjectiveQueryWrapper>,
    sender: &Addr,
) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.admin != sender {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

pub fn update_config(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    sender: Addr,
    admin: Option<Addr>,
    fee_recipient: Option<FeeRecipient>,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    verify_sender_is_admin(deps.as_ref(), &sender)?;
    let mut config = CONFIG.load(deps.storage)?;
    if let Some(admin) = admin {
        config.admin = admin;
    }
    if let Some(fee_recipient) = fee_recipient {
        config.fee_recipient = match fee_recipient {
            FeeRecipient::Address(addr) => addr,
            FeeRecipient::SwapContract => env.contract.address,
        };
    }
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("method", "update_config"))
}

pub fn withdraw_support_funds(
    deps: DepsMut<InjectiveQueryWrapper>,
    sender: Addr,
    coins: Vec<Coin>,
    target_address: Addr,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    verify_sender_is_admin(deps.as_ref(), &sender)?;
    let send_message = BankMsg::Send {
        to_address: target_address.to_string(),
        amount: coins,
    };
    let response = Response::new()
        .add_message(send_message)
        .add_attribute("method", "withdraw_support_funds")
        .add_attribute("target_address", target_address.to_string());
    Ok(response)
}


pub fn set_route(
    deps: DepsMut<InjectiveQueryWrapper>,
    sender: &Addr,
    source_denom: String,
    target_denom: String,
    route: Vec<MarketId>,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    verify_sender_is_admin(deps.as_ref(), sender)?;

    if source_denom == target_denom {
        return Err(ContractError::CustomError {
            val: "Cannot set a route with the same denom being source and target".to_string(),
        });
    }

    if route.is_empty() {
        return Err(ContractError::CustomError {
            val: "Route must have at least one step".to_string(),
        });
    }

    if route
        .clone()
        .into_iter()
        .collect::<HashSet<MarketId>>()
        .len()
        < route.len()
    {
        return Err(ContractError::CustomError {
            val: "Route cannot have duplicate steps!".to_string(),
        });
    }

    let route = SwapRoute {
        steps: route,
        source_denom,
        target_denom,
    };
    store_swap_route(deps.storage, &route)?;

    Ok(Response::new().add_attribute("method", "set_route"))
}

pub fn delete_route(
    deps: DepsMut<InjectiveQueryWrapper>,
    sender: &Addr,
    source_denom: String,
    target_denom: String,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    verify_sender_is_admin(deps.as_ref(), sender)?;

    remove_swap_route(deps.storage, &source_denom, &target_denom);

    Ok(Response::new().add_attribute("method", "delete_route"))
}
