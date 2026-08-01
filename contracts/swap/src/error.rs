use cosmwasm_std::StdError;
use injective_math::FPDecimal;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Custom Error: {val:?}")]
    CustomError { val: String },

    #[error("Failure response from submsg: {0}")]
    SubMsgFailure(String),

    #[error("Unrecognized reply id: {0}")]
    UnrecognizedReply(u64),

    #[error("Invalid reply from sub-message {id}, {err}")]
    ReplyParseFailure { id: u64, err: String },

    #[error("Min expected swap amount ({0}) not reached")]
    MinOutputAmountNotReached(FPDecimal),

    #[error("Provided amount of {0} is below required amount of {1}")]
    InsufficientFundsProvided(FPDecimal, FPDecimal),

    #[error("Atomic order consumed support funds for {denom}: minimum remaining {minimum_remaining}, actual remaining {actual_remaining}")]
    SupportFundsConsumed {
        denom: String,
        minimum_remaining: FPDecimal,
        actual_remaining: FPDecimal,
    },

    #[error("Contract can't be migrated")]
    MigrationError {},
}
