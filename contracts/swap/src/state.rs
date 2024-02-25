use cosmwasm_std::{Order, StdError, StdResult, Storage};
use cw_storage_plus::{Item, Map};

use crate::types::{Config, CurrentSwapOperation, CurrentSwapStep, SwapResults, SwapRoute};

pub const SWAP_ROUTES: Map<(String, String), SwapRoute> = Map::new("swap_routes");
pub const SWAP_OPERATION_STATE: Item<CurrentSwapOperation> = Item::new("current_swap_cache");
pub const STEP_STATE: Item<CurrentSwapStep> = Item::new("current_step_cache");
pub const SWAP_RESULTS: Item<Vec<SwapResults>> = Item::new("swap_results");
pub const CONFIG: Item<Config> = Item::new("config");

impl Config {
    pub fn validate(self) -> StdResult<()> {
        Ok(())
    }
}

pub fn store_swap_route(storage: &mut dyn Storage, route: &SwapRoute) -> StdResult<()> {
    let key = route_key(&route.source_denom, &route.target_denom);
    SWAP_ROUTES.save(storage, key, route)
}

pub fn read_swap_route(
    storage: &dyn Storage,
    source_denom: &str,
    target_denom: &str,
) -> StdResult<SwapRoute> {
    let key = route_key(source_denom, target_denom);
    SWAP_ROUTES.load(storage, key).map_err(|_| {
        StdError::generic_err(format!(
            "No swap route not found from {source_denom} to {target_denom}",
        ))
    })
}

pub fn get_config(storage: &dyn Storage) -> StdResult<Config> {
    let config = CONFIG.load(storage)?;
    Ok(config)
}

pub fn get_all_swap_routes(storage: &dyn Storage) -> StdResult<Vec<SwapRoute>> {
    let routes = SWAP_ROUTES
        .range(storage, None, None, Order::Ascending)
        .map(|item| item.unwrap().1)
        .collect();

    Ok(routes)
}

pub fn remove_swap_route(storage: &mut dyn Storage, source_denom: &str, target_denom: &str) {
    let key = route_key(source_denom, target_denom);
    SWAP_ROUTES.remove(storage, key)
}

fn route_key<'a>(source_denom: &'a str, target_denom: &'a str) -> (String, String) {
    if source_denom < target_denom {
        (source_denom.to_string(), target_denom.to_string())
    } else {
        (target_denom.to_string(), source_denom.to_string())
    }
}
