use cosmwasm_std::Addr;

use crate::admin::{delete_route, set_route};
use injective_cosmwasm::{inj_mock_deps, MarketId, OwnedDepsExt, TEST_MARKET_ID_1, TEST_MARKET_ID_2, TEST_MARKET_ID_3};

use crate::state::{read_swap_route, store_swap_route, CONFIG};
use crate::testing::test_utils::{mock_deps_eth_inj, MultiplierQueryBehavior, TEST_CONTRACT_ADDR, TEST_USER_ADDR};
use crate::types::{Config, SwapRoute};

#[test]
fn it_can_store_and_read_swap_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth";
    let target_denom = "inj";

    let route = SwapRoute {
        steps: vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)],
        source_denom: source_denom.to_string(),
        target_denom: target_denom.to_string(),
    };

    store_swap_route(deps.as_mut().storage, &route).unwrap();

    let stored_route = read_swap_route(&deps.storage, source_denom, target_denom).unwrap();
    assert_eq!(stored_route, route, "stored route was not read correctly");

    // Read with reversed denoms
    let stored_route_reversed = read_swap_route(&deps.storage, target_denom, source_denom).unwrap();
    assert_eq!(stored_route_reversed, route);

    let non_existent_route = read_swap_route(&deps.storage, "nonexistent", "route");
    assert!(non_existent_route.is_err(), "non-existent route was read");
}

#[test]
fn it_can_update_and_read_swap_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth";
    let target_denom = "usdt";

    let route = SwapRoute {
        steps: vec![MarketId::unchecked(TEST_MARKET_ID_1)],
        source_denom: source_denom.to_string(),
        target_denom: target_denom.to_string(),
    };

    store_swap_route(deps.as_mut().storage, &route).unwrap();

    let mut stored_route = read_swap_route(&deps.storage, source_denom, target_denom).unwrap();
    assert_eq!(stored_route, route, "stored route was not read correctly");

    let new_target_denom = "inj";

    let updated_route = SwapRoute {
        steps: vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)],
        source_denom: source_denom.to_string(),
        target_denom: new_target_denom.to_string(),
    };

    store_swap_route(deps.as_mut().storage, &updated_route).unwrap();

    stored_route = read_swap_route(&deps.storage, source_denom, new_target_denom).unwrap();
    assert_eq!(stored_route, updated_route, "stored route was not updated");
}

#[test]
fn owner_can_set_valid_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "inj".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route.clone(),
    );

    assert!(result.is_ok(), "result was not ok");

    let response = result.unwrap();
    assert_eq!(response.attributes[0].key, "method", "method attribute was not set");
    assert_eq!(response.attributes[0].value, "set_route", "method attribute was not set");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom).unwrap();
    assert_eq!(stored_route.steps, route, "route was not set correctly");
    assert_eq!(stored_route.source_denom, source_denom, "route was not set correctly");
    assert_eq!(stored_route.target_denom, target_denom, "route was not set correctly");
}

#[test]
fn owner_cannot_set_route_for_markets_using_target_denom_not_found_on_target_market() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "atom".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "result was ok");
    assert!(
        result.unwrap_err().to_string().contains("Target denom not found in last market"),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "route was set");
}

#[test]
fn owner_cannot_set_route_for_markets_using_source_denom_not_present_on_source_market() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "atom".to_string();
    let target_denom = "eth".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "result was ok");
    assert!(
        result.unwrap_err().to_string().contains("Source denom not found in first market"),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "route was set");
}

#[test]
fn owner_can_set_route_single_step_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "usdt".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route.clone(),
    );

    assert!(result.is_ok(), "result was not ok");

    let response = result.unwrap();
    assert_eq!(response.attributes[0].key, "method", "method attribute was not set");
    assert_eq!(response.attributes[0].value, "set_route", "method attribute was not set");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom).unwrap();
    assert_eq!(stored_route.steps, route, "route was not stored correctly");
    assert_eq!(stored_route.source_denom, source_denom, "source_denom was not stored correctly");
    assert_eq!(stored_route.target_denom, target_denom, "target_denom was not stored correctly");
}

#[test]
fn owner_can_set_route_single_step_route_with_reverted_denoms() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "usdt".to_string();
    let target_denom = "eth".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route.clone(),
    );

    assert!(result.is_ok(), "result was not ok");

    let response = result.unwrap();
    assert_eq!(response.attributes[0].key, "method", "method attribute was not set");
    assert_eq!(response.attributes[0].value, "set_route", "method attribute was not set");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom).unwrap();
    assert_eq!(stored_route.steps, route, "route was not stored correctly");
    assert_eq!(stored_route.source_denom, source_denom, "source_denom was not stored correctly");
    assert_eq!(stored_route.target_denom, target_denom, "target_denom was not stored correctly");
}

#[test]
fn it_returns_error_when_setting_route_for_the_same_denom_as_target_and_source() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "eth".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };

    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "Could set a route with the same denom being source and target!");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot set a route with the same denom being source and target"),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "Could read a route with the same denom being source and target!");
}

#[test]
fn it_returns_error_when_setting_route_with_nonexistent_market_id() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "usdt".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_3)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };

    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "Could set a route for non-existent market");
    let err_result = result.unwrap_err();

    assert!(
        err_result.to_string().contains(&format!("Market {TEST_MARKET_ID_3} not found")),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "Could read a route for non-existent market");
}

#[test]
fn it_returns_error_when_setting_route_with_no_market_ids() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "usdt".to_string();
    let route = vec![];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };

    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "Could set a route without any steps");
    assert!(
        result.unwrap_err().to_string().contains("Route must have at least one step"),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "Could read a route without any steps");
}

#[test]
fn it_returns_error_when_setting_route_with_duplicated_market_ids() {
    let mut deps = inj_mock_deps(|_| {});
    let source_denom = "eth".to_string();
    let target_denom = "usdt".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_1)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };

    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "Could set a route that begins and ends with the same market");
    assert!(
        result.unwrap_err().to_string().contains("Route cannot have duplicate steps"),
        "wrong error message"
    );

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_err(), "Could read a route that begins and ends with the same market");
}

#[test]
fn it_returns_error_if_non_admin_tries_to_set_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "inj".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_CONTRACT_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(result.is_err(), "expected error");
    assert!(result.unwrap_err().to_string().contains("Unauthorized"), "wrong error message");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom).unwrap_err();
    assert!(
        stored_route.to_string().contains("No swap route not found from eth to inj"),
        "wrong error message"
    );
}

#[test]
fn it_allows_admint_to_delete_existing_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "inj".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let set_result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(set_result.is_ok(), "expected success on set");

    let delete_result = delete_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
    );

    assert!(delete_result.is_ok(), "expected success on delete");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom).unwrap_err();
    assert!(
        stored_route.to_string().contains("No swap route not found from eth to inj"),
        "route was not deleted and could be read"
    );
}

#[test]
fn it_doesnt_fail_if_admin_deletes_non_existent_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "inj".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let set_result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(set_result.is_ok(), "expected success on set");

    let delete_result = delete_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        "mietek".to_string(),
    );

    assert!(delete_result.is_ok(), "expected success on delete");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_ok(), "route was deleted");
}

#[test]
fn it_returns_error_if_non_admin_tries_to_delete_route() {
    let mut deps = mock_deps_eth_inj(MultiplierQueryBehavior::Success);
    let source_denom = "eth".to_string();
    let target_denom = "inj".to_string();
    let route = vec![MarketId::unchecked(TEST_MARKET_ID_1), MarketId::unchecked(TEST_MARKET_ID_2)];

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    let set_result = set_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_USER_ADDR),
        source_denom.clone(),
        target_denom.clone(),
        route,
    );

    assert!(set_result.is_ok(), "expected success on set");

    let delete_result = delete_route(
        deps.as_mut(),
        &Addr::unchecked(TEST_CONTRACT_ADDR),
        source_denom.clone(),
        target_denom.clone(),
    );

    assert!(delete_result.is_err(), "expected error on delete");

    let stored_route = read_swap_route(&deps.storage, &source_denom, &target_denom);
    assert!(stored_route.is_ok(), "route was deleted");
}
