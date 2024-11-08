use crate::{
    msg::{FeeRecipient, InstantiateMsg, MigrateMsg},
    testing::{
        integration_realistic_tests_min_quantity::happy_path_two_hops_test,
        test_utils::{must_init_account_with_funds, str_coin, Decimals, ATOM, ETH, INJ, USDT},
    },
};

use cosmwasm_std::Addr;
use injective_std::types::cosmwasm::wasm::v1::{MsgMigrateContract, MsgMigrateContractResponse, QueryContractInfoRequest, QueryContractInfoResponse};
use injective_test_tube::{Account, ExecuteResponse, InjectiveTestApp, Module, Runner, Wasm};
use injective_testing::test_tube::utils::store_code;

type V101InstantiateMsg = InstantiateMsg;

#[test]
#[cfg_attr(not(feature = "integration"), ignore)]
fn test_migration() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);

    let wasm_byte_code = std::fs::read("../../contracts/swap/src/testing/test_artifacts/swap-contract-v101.wasm").unwrap();

    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let swap_v101_code_id = wasm.store_code(&wasm_byte_code, None, &owner).unwrap().data.code_id;

    let swap_v101_address: String = wasm
        .instantiate(
            swap_v101_code_id,
            &V101InstantiateMsg {
                admin: Addr::unchecked(owner.address()),
                fee_recipient: FeeRecipient::SwapContract,
            },
            Some(&owner.address()),
            Some("swap-contract"),
            &[str_coin("1_000", USDT, Decimals::Six)],
            &owner,
        )
        .unwrap()
        .data
        .address;

    let res: QueryContractInfoResponse = app
        .query(
            "/cosmwasm.wasm.v1.Query/ContractInfo",
            &QueryContractInfoRequest {
                address: swap_v101_address.clone(),
            },
        )
        .unwrap();
    let contract_info = res.contract_info.unwrap();

    assert_eq!(res.address, swap_v101_address);
    assert_eq!(contract_info.code_id, swap_v101_code_id);
    assert_eq!(contract_info.creator, owner.address());
    assert_eq!(contract_info.label, "swap-contract");

    let swap_v110_code_id = store_code(&wasm, &owner, "swap_contract".to_string());

    let _res: ExecuteResponse<MsgMigrateContractResponse> = app
        .execute(
            MsgMigrateContract {
                sender: owner.address(),
                contract: swap_v101_address.clone(),
                code_id: swap_v110_code_id,
                msg: serde_json_wasm::to_vec(&MigrateMsg {}).unwrap(),
            },
            "/cosmwasm.wasm.v1.MsgMigrateContract",
            &owner,
        )
        .unwrap();

    let res: QueryContractInfoResponse = app
        .query(
            "/cosmwasm.wasm.v1.Query/ContractInfo",
            &QueryContractInfoRequest {
                address: swap_v101_address.clone(),
            },
        )
        .unwrap();

    let contract_info = res.contract_info.unwrap();

    assert_eq!(res.address, swap_v101_address);
    assert_eq!(contract_info.code_id, swap_v110_code_id);
    assert_eq!(contract_info.creator, owner.address());
    assert_eq!(contract_info.label, "swap-contract");

    happy_path_two_hops_test(app, owner, swap_v101_address);
}
