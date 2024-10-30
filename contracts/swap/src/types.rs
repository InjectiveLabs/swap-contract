use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin};
use injective_cosmwasm::MarketId;
use injective_math::FPDecimal;

#[cw_serde]
pub enum SwapEstimationAmount {
    InputQuantity(FPCoin),
    ReceiveQuantity(FPCoin),
}

#[cw_serde]
pub struct FPCoin {
    pub amount: FPDecimal,
    pub denom: String,
}

impl From<FPCoin> for Coin {
    fn from(value: FPCoin) -> Self {
        Coin::new(value.amount, value.denom)
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

#[cw_serde]
pub struct ConfigResponse {
    pub config: Config,
    pub contract_version: String,
}

#[cw_serde]
pub enum SwapQuantityMode {
    MinOutputQuantity(FPDecimal),
    ExactOutputQuantity(FPDecimal),
}

#[cw_serde]
pub struct StepExecutionEstimate {
    pub worst_price: FPDecimal,
    pub result_denom: String,
    pub result_quantity: FPDecimal,
    pub is_buy_order: bool,
    pub fee_estimate: Option<FPCoin>,
}

#[cw_serde]
pub struct CurrentSwapOperation {
    // whole swap operation
    pub sender_address: Addr,
    pub swap_steps: Vec<MarketId>,
    pub swap_quantity_mode: SwapQuantityMode,
    pub input_funds: Coin,
    pub refund: Coin,
}

#[cw_serde]
pub struct CurrentSwapStep {
    // current step
    pub step_idx: u16,
    pub current_balance: FPCoin,
    pub step_target_denom: String,
    pub is_buy: bool,
}

#[cw_serde]
pub struct SwapResults {
    pub market_id: MarketId,
    pub quantity: FPDecimal,
    pub price: FPDecimal,
    pub fee: FPDecimal,
}

#[cw_serde]
pub struct Config {
    // if fee_recipient is contract, fee discount is replayed to a sender (will not stay in the contract)
    pub fee_recipient: Addr,
    // who can change routes
    pub admin: Addr,
}

#[cw_serde]
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

#[cw_serde]
pub struct SwapStep {
    pub market_id: MarketId,
    pub quote_denom: String, // quote for this step of swap, eg for swap eth/inj using eth/usdt and inj/usdt markets, quotes will be eth in 1st step and usdt in 2nd
}

#[cw_serde]
pub struct SwapEstimationResult {
    pub result_quantity: FPDecimal,
    pub expected_fees: Vec<FPCoin>,
}
