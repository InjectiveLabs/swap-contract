use cosmwasm_std::{StdError, StdResult, Storage};
use cw_storage_plus::{Item, Map};

use crate::types::{Config, CurrentSwapOperation, CurrentSwapStep, SwapRoute};

pub const SWAP_ROUTES: Map<(String, String), SwapRoute> = Map::new("swap_routes");
pub const SWAP_OPERATION_STATE: Item<CurrentSwapOperation> = Item::new("current_swap_cache");
pub const STEP_STATE: Item<CurrentSwapStep> = Item::new("current_step_cache");
pub const CONFIG: Item<Config> = Item::new("config");

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
            "No swap route not found from {} to {}",
            source_denom, target_denom
        ))
    })
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
