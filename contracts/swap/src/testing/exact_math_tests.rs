use crate::exact_math::{multiply_to_raw, ratio_to_tick, select_liquidity_levels, weighted_average_to_tick, LiquidityTarget, RoundingDirection};

use injective_cosmwasm::PriceLevel;
use injective_math::FPDecimal;
use primitive_types::{U256, U512};

fn raw(value: u128) -> FPDecimal {
    FPDecimal::from(U256::from(value))
}

fn level(price_raw: u128, quantity_raw: u128) -> PriceLevel {
    PriceLevel {
        p: raw(price_raw),
        q: raw(quantity_raw),
    }
}

#[test]
fn weighted_average_rounds_exact_values_around_a_tick() {
    let tick = raw(100);

    assert_eq!(weighted_average_to_tick(&[level(99, 1)], tick, RoundingDirection::Up).unwrap(), raw(100));
    assert_eq!(weighted_average_to_tick(&[level(100, 1)], tick, RoundingDirection::Up).unwrap(), raw(100));
    assert_eq!(weighted_average_to_tick(&[level(101, 1)], tick, RoundingDirection::Up).unwrap(), raw(200));
}

#[test]
fn weighted_average_preserves_the_code_67_witness_remainder() {
    let levels = vec![
        PriceLevel {
            p: raw(100),
            q: FPDecimal::must_from_str("10000"),
        },
        PriceLevel {
            p: raw(332_900),
            q: FPDecimal::must_from_str("0.03"),
        },
    ];

    assert_eq!(weighted_average_to_tick(&levels, raw(100), RoundingDirection::Up).unwrap(), raw(200));
    assert_eq!(weighted_average_to_tick(&levels, raw(100), RoundingDirection::Down).unwrap(), raw(100));
}

#[test]
fn weighted_average_does_not_discard_price_quantity_product_remainders() {
    let levels = vec![level(101, 1)];

    assert_eq!(weighted_average_to_tick(&levels, raw(100), RoundingDirection::Up).unwrap(), raw(200));
}

#[test]
fn notional_clipping_rounds_the_exact_terminal_quantity_to_its_tick() {
    let levels = vec![level(9_999_999_999_999_999, 1_000)];

    let selected = select_liquidity_levels(&levels, raw(1), LiquidityTarget::Notional, raw(100)).unwrap();

    assert_eq!(selected, vec![level(9_999_999_999_999_999, 200)]);
}

#[test]
fn quantity_clipping_rounds_directly_to_the_quantity_tick() {
    let levels = vec![level(100, 1_000)];

    let selected = select_liquidity_levels(&levels, raw(101), LiquidityTarget::Quantity, raw(100)).unwrap();

    assert_eq!(selected, vec![level(100, 200)]);
}

#[test]
fn ratio_rounds_directly_without_discarding_a_tick_boundary_remainder() {
    let result = ratio_to_tick(raw(1), raw(9_999_999_999_999_999), raw(100), RoundingDirection::Up).unwrap();

    assert_eq!(result, raw(200));
}

#[test]
fn multiplication_can_round_required_funds_up_at_raw_precision() {
    assert_eq!(multiply_to_raw(raw(1), raw(1), RoundingDirection::Down).unwrap(), FPDecimal::ZERO);
    assert_eq!(multiply_to_raw(raw(1), raw(1), RoundingDirection::Up).unwrap(), raw(1));
}

#[test]
fn weighted_average_upward_rounding_satisfies_the_exact_invariant() {
    for tick_raw in [1u128, 10, 100] {
        for first_tick_count in 1u128..=4 {
            for second_tick_count in first_tick_count..=6 {
                for first_quantity_raw in [1u128, 2, 7, 101] {
                    for second_quantity_raw in [1u128, 3, 11, 200] {
                        let levels = vec![
                            level(first_tick_count * tick_raw, first_quantity_raw),
                            level(second_tick_count * tick_raw, second_quantity_raw),
                        ];
                        let rounded = weighted_average_to_tick(&levels, raw(tick_raw), RoundingDirection::Up).unwrap();
                        let exact_notional = levels
                            .iter()
                            .fold(U512::zero(), |sum, item| sum.checked_add(item.p.num.full_mul(item.q.num)).unwrap());
                        let total_quantity = levels
                            .iter()
                            .fold(U512::zero(), |sum, item| sum.checked_add(U512::from(item.q.num)).unwrap());
                        let rounded_notional = U512::from(rounded.num).checked_mul(total_quantity).unwrap();

                        assert!(rounded_notional >= exact_notional);

                        let previous_tick = rounded.num - U256::from(tick_raw);
                        let previous_notional = U512::from(previous_tick).checked_mul(total_quantity).unwrap();
                        assert!(previous_notional < exact_notional);
                    }
                }
            }
        }
    }
}

#[test]
fn weighted_average_downward_rounding_satisfies_the_exact_invariant() {
    for tick_raw in [1u128, 10, 100] {
        for first_tick_count in 1u128..=4 {
            for second_tick_count in first_tick_count..=6 {
                for first_quantity_raw in [1u128, 2, 7, 101] {
                    for second_quantity_raw in [1u128, 3, 11, 200] {
                        let levels = vec![
                            level(first_tick_count * tick_raw, first_quantity_raw),
                            level(second_tick_count * tick_raw, second_quantity_raw),
                        ];
                        let rounded = weighted_average_to_tick(&levels, raw(tick_raw), RoundingDirection::Down).unwrap();
                        let exact_notional = levels
                            .iter()
                            .fold(U512::zero(), |sum, item| sum.checked_add(item.p.num.full_mul(item.q.num)).unwrap());
                        let total_quantity = levels
                            .iter()
                            .fold(U512::zero(), |sum, item| sum.checked_add(U512::from(item.q.num)).unwrap());
                        let rounded_notional = U512::from(rounded.num).checked_mul(total_quantity).unwrap();

                        assert!(rounded_notional <= exact_notional);

                        let next_tick = rounded.num + U256::from(tick_raw);
                        let next_notional = U512::from(next_tick).checked_mul(total_quantity).unwrap();
                        assert!(next_notional > exact_notional);
                    }
                }
            }
        }
    }
}

#[test]
fn exact_arithmetic_returns_errors_instead_of_overflowing() {
    let maximum = FPDecimal::from(U256::MAX);
    let levels = vec![PriceLevel { p: maximum, q: maximum }, PriceLevel { p: maximum, q: maximum }];

    let error = weighted_average_to_tick(&levels, raw(1), RoundingDirection::Up).unwrap_err();

    assert!(error.to_string().contains("weighted notional overflow"));
}
