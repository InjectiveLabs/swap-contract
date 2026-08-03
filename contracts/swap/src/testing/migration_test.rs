use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{FeeRecipient, InstantiateMsg, MigrateMsg, QueryMsg},
    state::{read_swap_route, store_swap_route, CONFIG},
    testing::{
        integration_realistic_tests_min_quantity::happy_path_two_hops_test,
        test_utils::{
            initial_coin, launch_realistic_usdt_usdc_spot_market, must_init_account_with_funds_and_setting_denoms, query_bank_balance,
            set_route_and_assert_success, str_coin, Decimals, ATOM, ETH, INJ, USDC, USDT,
        },
    },
    types::{Config, ConfigResponse, SwapRoute},
    ContractError,
};

use cosmwasm_std::{testing::mock_env, Addr};
use cw2::{get_contract_version, set_contract_version};
use injective_cosmwasm::{inj_mock_deps, MarketId, TEST_MARKET_ID_1};
use injective_math::FPDecimal;
use injective_std::types::cosmwasm::wasm::v1::{QueryContractInfoRequest, QueryContractInfoResponse};
use injective_test_tube::{Account, Bank, Exchange, InjectiveTestApp, Module, Runner, Wasm};

const CODE_67_CW2_NAME: &str = "crates.io:atomic-order-example";
const CODE_67_CW2_VERSION: &str = "0.1.0";
const CODE_1962_CW2_NAME: &str = "swap-contract";
const CODE_1962_CW2_VERSION: &str = "1.1.2";

#[test]
fn migrates_persistent_state_from_code_67() {
    assert_storage_migration(CODE_67_CW2_NAME, CODE_67_CW2_VERSION, "code_67");
}

#[test]
fn migrates_persistent_state_from_code_1962() {
    assert_storage_migration(CODE_1962_CW2_NAME, CODE_1962_CW2_VERSION, "code_1962");
}

#[test]
fn rejects_unknown_migration_source() {
    let mut deps = inj_mock_deps(|_| {});
    set_contract_version(deps.as_mut().storage, "swap-contract", "9.9.9").unwrap();

    let result = migrate(deps.as_mut(), mock_env(), MigrateMsg {});

    assert!(matches!(result, Err(ContractError::MigrationError {})));
}

#[test]
#[cfg_attr(not(feature = "integration"), ignore)]
fn migrates_the_deployed_code_67_artifact() {
    assert_deployed_artifact_migrates("mainnet_code_67.wasm");
}

#[test]
#[cfg_attr(not(feature = "integration"), ignore)]
fn migrates_the_deployed_code_1962_artifact() {
    assert_deployed_artifact_migrates("mainnet_code_1962.wasm");
}

fn assert_storage_migration(previous_name: &str, previous_version: &str, expected_source: &str) {
    let mut deps = inj_mock_deps(|_| {});
    let expected_config = Config {
        fee_recipient: Addr::unchecked("fee_recipient"),
        admin: Addr::unchecked("admin"),
    };
    let expected_route = SwapRoute {
        steps: vec![MarketId::unchecked(TEST_MARKET_ID_1)],
        source_denom: "source".to_string(),
        target_denom: "target".to_string(),
    };

    CONFIG.save(deps.as_mut().storage, &expected_config).unwrap();
    store_swap_route(deps.as_mut().storage, &expected_route).unwrap();
    set_contract_version(deps.as_mut().storage, previous_name, previous_version).unwrap();

    let response = migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();

    assert_eq!(CONFIG.load(&deps.storage).unwrap(), expected_config);
    assert_eq!(read_swap_route(&deps.storage, "source", "target").unwrap(), expected_route);
    assert_eq!(
        get_contract_version(&deps.storage).unwrap(),
        cw2::ContractVersion {
            contract: CONTRACT_NAME.to_string(),
            version: CONTRACT_VERSION.to_string(),
        }
    );
    assert!(response
        .attributes
        .iter()
        .any(|attribute| attribute.key == "migration_source" && attribute.value == expected_source));
}

fn assert_deployed_artifact_migrates(fixture_name: &str) {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);
    let exchange = Exchange::new(&app);
    let validator = app.get_first_validator_signing_account(INJ.to_string(), 1.2f64).unwrap();
    let owner = must_init_account_with_funds_and_setting_denoms(
        &app,
        &validator,
        &[
            initial_coin("1", ETH, Decimals::Eighteen),
            initial_coin("1", ATOM, Decimals::Six),
            initial_coin("1_000", USDT, Decimals::Six),
            initial_coin("1", USDC, Decimals::Six),
            initial_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );
    let persisted_market_id = launch_realistic_usdt_usdc_spot_market(&exchange, &owner);
    let fixture_path = format!("{}/src/testing/test_artifacts/{fixture_name}", env!("CARGO_MANIFEST_DIR"));
    let previous_wasm = std::fs::read(fixture_path).unwrap();
    let previous_code_id = wasm.store_code(&previous_wasm, None, &owner).unwrap().data.code_id;
    let contract_address = wasm
        .instantiate(
            previous_code_id,
            &InstantiateMsg {
                admin: Addr::unchecked(owner.address()),
                fee_recipient: FeeRecipient::SwapContract,
            },
            Some(&owner.address()),
            Some("swap-contract-migration-test"),
            &[str_coin("100", USDT, Decimals::Six)],
            &owner,
        )
        .unwrap()
        .data
        .address;
    let balance_before = query_bank_balance(&bank, USDT, &contract_address);
    set_route_and_assert_success(&wasm, &owner, &contract_address, USDT, USDC, vec![persisted_market_id.as_str().into()]);
    let route_before: SwapRoute = wasm
        .query(
            &contract_address,
            &QueryMsg::GetRoute {
                source_denom: USDT.to_string(),
                target_denom: USDC.to_string(),
            },
        )
        .unwrap();

    let release_wasm_path = format!("{}/../../artifacts/swap_contract.wasm", env!("CARGO_MANIFEST_DIR"));
    let release_wasm = std::fs::read(release_wasm_path).unwrap();
    let new_code_id = wasm.store_code(&release_wasm, None, &owner).unwrap().data.code_id;
    wasm.migrate(new_code_id, &contract_address, &MigrateMsg {}, &owner).unwrap();

    let contract_info: QueryContractInfoResponse = app
        .query(
            "/cosmwasm.wasm.v1.Query/ContractInfo",
            &QueryContractInfoRequest {
                address: contract_address.clone(),
            },
        )
        .unwrap();
    let config: ConfigResponse = wasm.query(&contract_address, &QueryMsg::GetConfig {}).unwrap();
    let route_after: SwapRoute = wasm
        .query(
            &contract_address,
            &QueryMsg::GetRoute {
                source_denom: USDT.to_string(),
                target_denom: USDC.to_string(),
            },
        )
        .unwrap();

    assert_eq!(contract_info.contract_info.unwrap().code_id, new_code_id);
    assert_eq!(config.contract_version, CONTRACT_VERSION);
    assert_eq!(config.config.admin, Addr::unchecked(owner.address()));
    assert_eq!(config.config.fee_recipient, Addr::unchecked(contract_address.clone()));
    assert_eq!(query_bank_balance(&bank, USDT, &contract_address), balance_before);
    assert_eq!(balance_before, FPDecimal::from(100_000_000u128));
    assert_eq!(route_after, route_before);

    happy_path_two_hops_test(app, owner, contract_address);
}
