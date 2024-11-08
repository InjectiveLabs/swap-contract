use cosmwasm_std::testing::mock_env;
use cosmwasm_std::Addr;

use injective_cosmwasm::{MarketId, OwnedDepsExt, TEST_MARKET_ID_1, TEST_MARKET_ID_2};

use crate::admin::set_route;
use crate::queries::estimate_single_swap_execution;
use crate::state::CONFIG;
use crate::testing::test_utils::{mock_deps_eth_inj, str_coin, Decimals, MultiplierQueryBehavior, TEST_USER_ADDR};
use crate::types::{Config, FPCoin, SwapEstimationAmount};

#[test]
fn it_reverts_if_atomic_fee_multiplier_query_fails() {
    let env = mock_env();
    let deps_binding = mock_deps_eth_inj(MultiplierQueryBehavior::Fail);
    let mut deps = deps_binding;

    let config = Config {
        fee_recipient: Addr::unchecked(TEST_USER_ADDR),
        admin: Addr::unchecked(TEST_USER_ADDR),
    };
    CONFIG.save(deps.as_mut_deps().storage, &config).expect("could not save config");

    set_route(
        deps.as_mut_deps(),
        &Addr::unchecked(TEST_USER_ADDR),
        "eth".to_string(),
        "inj".to_string(),
        vec![TEST_MARKET_ID_1.into(), TEST_MARKET_ID_2.into()],
    )
    .unwrap();

    let response_1 = estimate_single_swap_execution(
        &deps.as_mut_deps().as_ref(),
        &env,
        &MarketId::unchecked(TEST_MARKET_ID_1.to_string()),
        SwapEstimationAmount::InputQuantity(FPCoin::from(str_coin("1", "eth", Decimals::Eighteen))),
        true, // is_simulation
    );

    assert!(response_1.is_err(), "should have failed");
    assert!(
        response_1.unwrap_err().to_string().contains("Querier system error: Unknown system error"),
        "wrong error message"
    );
}
