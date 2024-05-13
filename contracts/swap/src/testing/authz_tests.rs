use crate::{
    msg::ExecuteMsg,
    testing::test_utils::{
        create_contract_authorization, create_realistic_atom_usdt_sell_orders_from_spreadsheet,
        create_realistic_eth_usdt_buy_orders_from_spreadsheet, init_rich_account,
        init_self_relaying_contract_and_get_address, launch_realistic_atom_usdt_spot_market,
        launch_realistic_weth_usdt_spot_market, must_init_account_with_funds, str_coin, Decimals,
        ATOM, ETH, INJ, USDT,
    },
};

use cosmos_sdk_proto::{cosmwasm::wasm::v1::MsgExecuteContract, traits::MessageExt};
use injective_std::{shim::Any, types::cosmos::authz::v1beta1::MsgExec};
use injective_test_tube::{Account, Authz, Exchange, InjectiveTestApp, Module, Wasm};

#[test]
pub fn set_route_for_third_party_test() {
    let app = InjectiveTestApp::new();
    let wasm = Wasm::new(&app);
    let authz = Authz::new(&app);
    let exchange = Exchange::new(&app);

    let owner = must_init_account_with_funds(
        &app,
        &[
            str_coin("1", ETH, Decimals::Eighteen),
            str_coin("1", ATOM, Decimals::Six),
            str_coin("1_000", USDT, Decimals::Six),
            str_coin("10_000", INJ, Decimals::Eighteen),
        ],
    );

    let spot_market_1_id = launch_realistic_weth_usdt_spot_market(&exchange, &owner);
    let spot_market_2_id = launch_realistic_atom_usdt_spot_market(&exchange, &owner);

    let contr_addr = init_self_relaying_contract_and_get_address(
        &wasm,
        &owner,
        &[str_coin("1_000", USDT, Decimals::Six)],
    );

    let trader1 = init_rich_account(&app);
    let trader2 = init_rich_account(&app);
    let trader3 = init_rich_account(&app);

    create_contract_authorization(
        &app,
        contr_addr.clone(),
        &owner,
        trader1.address().to_string(),
        "set_route".to_string(),
        1,
        None,
    );

    create_realistic_eth_usdt_buy_orders_from_spreadsheet(
        &app,
        &spot_market_1_id,
        &trader1,
        &trader2,
    );
    create_realistic_atom_usdt_sell_orders_from_spreadsheet(
        &app,
        &spot_market_2_id,
        &trader1,
        &trader2,
        &trader3,
    );

    app.increase_time(1);

    let set_route_msg = ExecuteMsg::SetRoute {
        source_denom: ETH.to_string(),
        target_denom: ATOM.to_string(),
        route: vec![
            spot_market_1_id.as_str().into(),
            spot_market_2_id.as_str().into(),
        ],
    };

    let execute_msg = MsgExecuteContract {
        contract: contr_addr.clone(),
        sender: owner.address().to_string(),
        msg: serde_json_wasm::to_vec(&set_route_msg).unwrap(),
        funds: vec![],
    };

    // execute on more time to excercise account sequence
    let msg = MsgExec {
        grantee: trader1.address().to_string(),
        msgs: vec![Any {
            type_url: "/cosmwasm.wasm.v1.MsgExecuteContract".to_string(),
            value: execute_msg.to_bytes().unwrap(),
        }],
    };

    let _res = authz.exec(msg, &trader1).unwrap();

    // execute on more time to excercise account sequence
    let msg = MsgExec {
        grantee: trader1.address().to_string(),
        msgs: vec![Any {
            type_url: "/cosmwasm.wasm.v1.MsgExecuteContract".to_string(),
            value: execute_msg.to_bytes().unwrap(),
        }],
    };

    let err = authz.exec(msg, &trader1).unwrap_err();
    assert!(
        err.to_string().contains("failed to update grant with key"),
        "incorrect error returned by execute"
    );
}
