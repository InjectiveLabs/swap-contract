use crate::types::FPCoin;
use cosmwasm_std::Coin;
use injective_math::FPDecimal;

#[test]
fn fp_coin_conversion_uses_integer_units_not_fixed_point_raw_units() {
    let coin = FPCoin {
        amount: FPDecimal::must_from_str("4250000.9"),
        denom: "atom".to_string(),
    }
    .to_coin_floor()
    .unwrap();

    assert_eq!(coin, Coin::new(4_250_000u128, "atom"));
}

#[test]
fn fp_coin_conversion_rejects_negative_amounts() {
    let result = FPCoin {
        amount: FPDecimal::must_from_str("-1"),
        denom: "atom".to_string(),
    }
    .to_coin_floor();

    assert!(result.is_err());
}
