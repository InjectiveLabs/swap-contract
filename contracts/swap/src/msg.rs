use cosmwasm_std::{Addr, Coin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use injective_cosmwasm::MarketId;
use injective_math::FPDecimal;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeeRecipient {
    Address(Addr),
    SwapContract,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub fee_recipient: FeeRecipient,
    pub admin: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SwapMinOutput {
        target_denom: String,
        min_output_quantity: FPDecimal,
    },
    SwapExactOutput {
        target_denom: String,
        target_output_quantity: FPDecimal,
    },
    SetRoute {
        source_denom: String,
        target_denom: String,
        route: Vec<MarketId>,
    },
    DeleteRoute {
        source_denom: String,
        target_denom: String,
    },
    UpdateConfig {
        admin: Option<Addr>,
        fee_recipient: Option<FeeRecipient>,
    },
    WithdrawSupportFunds {
        coins: Vec<Coin>,
        target_address: Addr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetRoute {
        source_denom: String,
        target_denom: String,
    },
    GetOutputQuantity {
        from_quantity: FPDecimal,
        source_denom: String,
        target_denom: String,
    },
    GetInputQuantity {
        to_quantity: FPDecimal,
        source_denom: String,
        target_denom: String,
    },
    GetAllRoutes {},
    GetConfig {},
}
