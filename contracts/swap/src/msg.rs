use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin};
use injective_cosmwasm::MarketId;
use injective_math::FPDecimal;

#[cw_serde]
pub enum FeeRecipient {
    Address(Addr),
    SwapContract,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub fee_recipient: FeeRecipient,
    pub admin: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
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

#[cw_serde]
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
    GetAllRoutes {
        start_after: Option<(String, String)>,
        limit: Option<u32>,
    },
    GetConfig {},
}
