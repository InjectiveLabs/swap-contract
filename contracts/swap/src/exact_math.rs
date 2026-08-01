use cosmwasm_std::{StdError, StdResult};
use injective_cosmwasm::PriceLevel;
use injective_math::FPDecimal;
use primitive_types::{U256, U512};

#[derive(Clone, Copy)]
pub(crate) enum RoundingDirection {
    Down,
    Up,
}

#[derive(Clone, Copy)]
pub(crate) enum LiquidityTarget {
    Quantity,
    Notional,
}

pub(crate) fn select_liquidity_levels(
    levels: &[PriceLevel],
    total: FPDecimal,
    target: LiquidityTarget,
    min_quantity_tick_size: FPDecimal,
) -> StdResult<Vec<PriceLevel>> {
    ensure_positive(total, "liquidity target")?;
    ensure_positive(min_quantity_tick_size, "minimum quantity tick size")?;

    let target_raw = match target {
        LiquidityTarget::Quantity => U512::from(total.num),
        LiquidityTarget::Notional => total.num.full_mul(FPDecimal::ONE.num),
    };
    let mut consumed = U512::zero();
    let mut selected = Vec::new();

    for level in levels {
        ensure_positive(level.p, "price level price")?;
        ensure_positive(level.q, "price level quantity")?;

        let level_value = match target {
            LiquidityTarget::Quantity => U512::from(level.q.num),
            LiquidityTarget::Notional => level.p.num.full_mul(level.q.num),
        };
        let remaining = target_raw
            .checked_sub(consumed)
            .ok_or_else(|| arithmetic_error("liquidity target subtraction underflow"))?;

        if level_value < remaining {
            consumed = consumed
                .checked_add(level_value)
                .ok_or_else(|| arithmetic_error("liquidity accumulation overflow"))?;
            selected.push(level.clone());
            continue;
        }

        if level_value == remaining {
            selected.push(level.clone());
            return Ok(selected);
        }

        let required_quantity_raw = match target {
            LiquidityTarget::Quantity => remaining,
            LiquidityTarget::Notional => checked_ceil_div(remaining, U512::from(level.p.num))?,
        };
        let rounded_quantity_raw = checked_round_up_to_multiple(required_quantity_raw, U512::from(min_quantity_tick_size.num))?;

        if rounded_quantity_raw > U512::from(level.q.num) {
            return Err(arithmetic_error("rounded terminal quantity exceeds available liquidity"));
        }

        selected.push(PriceLevel {
            p: level.p,
            q: FPDecimal::from(to_u256(rounded_quantity_raw)?),
        });
        return Ok(selected);
    }

    Err(StdError::msg("Not enough liquidity to fulfill order"))
}

pub(crate) fn weighted_average_to_tick(levels: &[PriceLevel], min_price_tick_size: FPDecimal, direction: RoundingDirection) -> StdResult<FPDecimal> {
    ensure_positive(min_price_tick_size, "minimum price tick size")?;

    let mut weighted_notional = U512::zero();
    let mut total_quantity = U512::zero();

    for level in levels {
        ensure_positive(level.p, "price level price")?;
        ensure_positive(level.q, "price level quantity")?;

        weighted_notional = weighted_notional
            .checked_add(level.p.num.full_mul(level.q.num))
            .ok_or_else(|| arithmetic_error("weighted notional overflow"))?;
        total_quantity = total_quantity
            .checked_add(U512::from(level.q.num))
            .ok_or_else(|| arithmetic_error("total quantity overflow"))?;
    }

    if total_quantity.is_zero() {
        return Err(arithmetic_error("total quantity must be positive"));
    }

    let tick_denominator = total_quantity
        .checked_mul(U512::from(min_price_tick_size.num))
        .ok_or_else(|| arithmetic_error("weighted average tick denominator overflow"))?;
    let tick_count = match direction {
        RoundingDirection::Down => weighted_notional / tick_denominator,
        RoundingDirection::Up => checked_ceil_div(weighted_notional, tick_denominator)?,
    };
    let rounded_raw = tick_count
        .checked_mul(U512::from(min_price_tick_size.num))
        .ok_or_else(|| arithmetic_error("rounded weighted average overflow"))?;

    if rounded_raw.is_zero() {
        return Err(arithmetic_error("rounded weighted average must be positive"));
    }

    Ok(FPDecimal::from(to_u256(rounded_raw)?))
}

pub(crate) fn ratio_to_tick(numerator: FPDecimal, denominator: FPDecimal, tick: FPDecimal, direction: RoundingDirection) -> StdResult<FPDecimal> {
    ensure_positive(numerator, "ratio numerator")?;
    ensure_positive(denominator, "ratio denominator")?;
    ensure_positive(tick, "ratio tick")?;

    let scaled_numerator = numerator.num.full_mul(FPDecimal::ONE.num);
    let scaled_denominator = denominator.num.full_mul(tick.num);
    let tick_count = match direction {
        RoundingDirection::Down => scaled_numerator / scaled_denominator,
        RoundingDirection::Up => checked_ceil_div(scaled_numerator, scaled_denominator)?,
    };
    let rounded_raw = tick_count
        .checked_mul(U512::from(tick.num))
        .ok_or_else(|| arithmetic_error("rounded ratio overflow"))?;

    Ok(FPDecimal::from(to_u256(rounded_raw)?))
}

pub(crate) fn multiply_to_raw(left: FPDecimal, right: FPDecimal, direction: RoundingDirection) -> StdResult<FPDecimal> {
    ensure_non_negative(left, "multiplication left operand")?;
    ensure_non_negative(right, "multiplication right operand")?;

    let product = left.num.full_mul(right.num);
    let scale = U512::from(FPDecimal::ONE.num);
    let result_raw = match direction {
        RoundingDirection::Down => product / scale,
        RoundingDirection::Up => checked_ceil_div(product, scale)?,
    };

    Ok(FPDecimal::from(to_u256(result_raw)?))
}

fn checked_ceil_div(numerator: U512, denominator: U512) -> StdResult<U512> {
    if denominator.is_zero() {
        return Err(arithmetic_error("division by zero"));
    }

    let quotient = numerator / denominator;
    if numerator % denominator == U512::zero() {
        Ok(quotient)
    } else {
        quotient
            .checked_add(U512::one())
            .ok_or_else(|| arithmetic_error("division ceiling overflow"))
    }
}

fn checked_round_up_to_multiple(value: U512, tick: U512) -> StdResult<U512> {
    checked_ceil_div(value, tick)?
        .checked_mul(tick)
        .ok_or_else(|| arithmetic_error("tick rounding overflow"))
}

fn ensure_positive(value: FPDecimal, name: &str) -> StdResult<()> {
    if value.is_zero() || value.is_negative() {
        return Err(arithmetic_error(format!("{name} must be positive")));
    }
    Ok(())
}

fn ensure_non_negative(value: FPDecimal, name: &str) -> StdResult<()> {
    if value.is_negative() {
        return Err(arithmetic_error(format!("{name} must not be negative")));
    }
    Ok(())
}

fn to_u256(value: U512) -> StdResult<U256> {
    U256::try_from(value).map_err(|_| arithmetic_error("result does not fit in FPDecimal"))
}

fn arithmetic_error(message: impl Into<String>) -> StdError {
    StdError::msg(format!("exact arithmetic error: {}", message.into()))
}
