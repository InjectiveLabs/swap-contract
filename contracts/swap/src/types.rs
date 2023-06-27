use cosmwasm_std::{Addr, Coin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use injective_cosmwasm::MarketId;
use injective_math::FPDecimal;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub enum SwapEstimationAmount {
    InputQuantity(FPCoin),
    ReceiveQuantity(FPCoin),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct FPCoin {
    pub amount: FPDecimal,
    pub denom: String,
}

impl From<FPCoin> for Coin {
    fn from(value: FPCoin) -> Self {
        Coin::new(value.amount.into(), value.denom)
    }
}

impl From<Coin> for FPCoin {
    fn from(value: Coin) -> Self {
        FPCoin {
            amount: value.amount.into(),
            denom: value.denom,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct StepExecutionEstimate {
    pub worst_price: FPDecimal,
    pub result_denom: String,
    pub result_quantity: FPDecimal,
    pub is_buy_order: bool,
    pub fee_estimate: Option<FPCoin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct CurrentSwapOperation {
    // whole swap operation
    pub sender_address: Addr,
    pub swap_steps: Vec<MarketId>,
    pub min_target_quantity: FPDecimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct CurrentSwapStep {
    // current step
    pub step_idx: u16,
    pub current_balance: FPCoin,
    pub step_target_denom: String,
    pub is_buy: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    // if fee_recipient is contract, fee discount is replayed to a sender (will not stay in the contract)
    pub fee_recipient: Addr,
    // who can change routes
    pub admin: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SwapRoute {
    pub steps: Vec<MarketId>,
    pub source_denom: String,
    pub target_denom: String,
}

impl SwapRoute {
    pub fn steps_from(&self, denom: &str) -> Vec<MarketId> {
        if self.source_denom == denom {
            self.steps.clone()
        } else {
            let mut mut_steps = self.steps.clone();
            mut_steps.reverse();
            mut_steps
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SwapStep {
    pub market_id: MarketId,
    pub quote_denom: String, // quote for this step of swap, eg for swap eth/inj using eth/usdt and inj/usdt markets, quotes will be eth in 1st step and usdt in 2nd
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SwapEstimationResult {
    pub result_quantity: FPDecimal,
    pub expected_fees: Vec<FPCoin>,
}
