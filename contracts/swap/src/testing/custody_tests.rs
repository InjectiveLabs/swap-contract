use crate::{
    swap::{ensure_minimum_output, ensure_support_funds_preserved},
    ContractError,
};

use cosmwasm_std::Coin;
use injective_math::FPDecimal;

#[test]
fn support_floor_accepts_an_authorized_input_debit() {
    assert!(ensure_support_funds_preserved("usdt", FPDecimal::from(100u128), FPDecimal::from(100u128)).is_ok());
    assert!(ensure_support_funds_preserved("usdt", FPDecimal::from(100u128), FPDecimal::from(101u128)).is_ok());
}

#[test]
fn support_floor_rejects_any_debit_beyond_the_authorized_input() {
    let result = ensure_support_funds_preserved("usdt", FPDecimal::from(100u128), FPDecimal::must_from_str("99.999999999999999999"));

    assert!(matches!(
        result,
        Err(ContractError::SupportFundsConsumed {
            denom,
            minimum_remaining,
            actual_remaining,
        }) if denom == "usdt"
            && minimum_remaining == FPDecimal::from(100u128)
            && actual_remaining == FPDecimal::must_from_str("99.999999999999999999")
    ));
}

#[test]
fn minimum_output_is_checked_against_integer_bank_units() {
    let output = Coin::new(10u128, "atom");
    let minimum = FPDecimal::must_from_str("10.5");

    assert!(matches!(
        ensure_minimum_output(&output, minimum),
        Err(ContractError::MinOutputAmountNotReached(value)) if value == minimum
    ));
    assert!(ensure_minimum_output(&output, FPDecimal::from(10u128)).is_ok());
}
