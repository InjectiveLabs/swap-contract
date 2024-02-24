use crate::msg::FeeRecipient;
use crate::state::{remove_swap_route, store_swap_route, CONFIG};
use crate::types::{Config, SwapRoute};
use crate::ContractError;
use crate::ContractError::CustomError;
use cosmwasm_std::{
    ensure, ensure_eq, Addr, Attribute, BankMsg, Coin, Deps, DepsMut, Env, Event, Response,
    StdResult,
};
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQuerier, InjectiveQueryWrapper, MarketId};
use std::collections::HashSet;

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
    config.to_owned().validate()?;

    CONFIG.save(deps.storage, &config)
}

pub fn verify_sender_is_admin(
    deps: Deps<InjectiveQueryWrapper>,
    sender: &Addr,
) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure_eq!(&config.admin, sender, ContractError::Unauthorized {});
    Ok(())
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
    let mut updated_config_event_attrs: Vec<Attribute> = Vec::new();
    if let Some(admin) = admin {
        config.admin = admin.clone();
        updated_config_event_attrs.push(Attribute::new("admin", admin.to_string()));
    }
    if let Some(fee_recipient) = fee_recipient {
        config.fee_recipient = match fee_recipient {
            FeeRecipient::Address(addr) => addr,
            FeeRecipient::SwapContract => env.contract.address,
        };
        updated_config_event_attrs.push(Attribute::new(
            "fee_recipient",
            config.fee_recipient.to_string(),
        ));
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "update_config")
        .add_event(Event::new("config_updated").add_attributes(updated_config_event_attrs)))
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
    verify_route_exists(deps.as_ref(), &route)?;
    store_swap_route(deps.storage, &route)?;

    Ok(Response::new().add_attribute("method", "set_route"))
}

fn verify_route_exists(
    deps: Deps<InjectiveQueryWrapper>,
    route: &SwapRoute,
) -> Result<(), ContractError> {
    struct MarketDenom {
        quote_denom: String,
        base_denom: String,
    }
    let mut denoms: Vec<MarketDenom> = Vec::new();
    let querier = InjectiveQuerier::new(&deps.querier);

    for market_id in route.steps.iter() {
        let market = querier
            .query_spot_market(market_id)?
            .market
            .ok_or(CustomError {
                val: format!("Market {} not found", market_id.as_str()).to_string(),
            })?;

        denoms.push(MarketDenom {
            quote_denom: market.quote_denom,
            base_denom: market.base_denom,
        })
    }

    // defensive programming
    ensure!(
        !denoms.is_empty(),
        CustomError {
            val: "No market denoms found".to_string()
        }
    );
    ensure!(
        denoms.first().unwrap().quote_denom == route.source_denom
            || denoms.first().unwrap().base_denom == route.source_denom,
        CustomError {
            val: "Source denom not found in first market".to_string()
        }
    );
    ensure!(
        denoms.last().unwrap().quote_denom == route.target_denom
            || denoms.last().unwrap().base_denom == route.target_denom,
        CustomError {
            val: "Target denom not found in last market".to_string()
        }
    );

    Ok(())
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
