use cosmwasm_std::{CosmosMsg, DepsMut, Response, SubMsg};

use cw_storage_plus::Item;
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper};
use injective_math::FPDecimal;

use crate::{state::CONFIG, types::Config, ContractError};

pub fn i32_to_dec(source: i32) -> FPDecimal {
    FPDecimal::from(i128::from(source))
}

pub fn get_message_data(
    response: &[SubMsg<InjectiveMsgWrapper>],
    position: usize,
) -> &InjectiveMsgWrapper {
    let sth = match &response.get(position).unwrap().msg {
        CosmosMsg::Custom(msg) => msg,
        _ => panic!("No wrapped message found"),
    };
    sth
}

pub fn round_up_to_min_tick(num: FPDecimal, min_tick: FPDecimal) -> FPDecimal {
    if num < min_tick {
        return min_tick;
    }

    let remainder = FPDecimal::from(num.num % min_tick.num);

    if remainder.num.is_zero() {
        return num;
    }

    FPDecimal::from(num.num - remainder.num + min_tick.num)
}

pub trait Scaled {
    fn scaled(self, digits: i32) -> Self;
}

impl Scaled for FPDecimal {
    fn scaled(self, digits: i32) -> Self {
        self.to_owned()
            * FPDecimal::from(10i128)
                .pow(FPDecimal::from(digits as i128))
                .unwrap()
    }
}

pub fn dec_scale_factor() -> FPDecimal {
    FPDecimal::ONE.scaled(18)
}

type V100Config = Config;
const V100CONFIG: Item<V100Config> = Item::new("config");

pub fn handle_config_migration(
    deps: DepsMut<InjectiveQueryWrapper>,
) -> Result<Response, ContractError> {
    let v100_config = V100CONFIG.load(deps.storage)?;

    let config = Config {
        fee_recipient: v100_config.fee_recipient,
        admin: v100_config.admin,
    };

    CONFIG.save(deps.storage, &config)?;

    config.validate()?;

    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_descale() {
        let val = FPDecimal::must_from_str("1000000000000000000");
        let descaled = val.scaled(-18);
        assert_eq!(descaled, FPDecimal::from(1u128));
        let scaled = descaled.scaled(18);
        assert_eq!(scaled, val);
    }

    #[test]
    fn test_round_up_to_min_tick() {
        let num = FPDecimal::from(37u128);
        let min_tick = FPDecimal::from(10u128);

        let result = round_up_to_min_tick(num, min_tick);
        assert_eq!(result, FPDecimal::from(40u128));

        let num = FPDecimal::from_str("0.00000153").unwrap();
        let min_tick = FPDecimal::from_str("0.000001").unwrap();

        let result = round_up_to_min_tick(num, min_tick);
        assert_eq!(result, FPDecimal::from_str("0.000002").unwrap());

        let num = FPDecimal::from_str("0.000001").unwrap();
        let min_tick = FPDecimal::from_str("0.000001").unwrap();

        let result = round_up_to_min_tick(num, min_tick);
        assert_eq!(result, FPDecimal::from_str("0.000001").unwrap());

        let num = FPDecimal::from_str("0.0000001").unwrap();
        let min_tick = FPDecimal::from_str("0.000001").unwrap();

        let result = round_up_to_min_tick(num, min_tick);
        assert_eq!(result, FPDecimal::from_str("0.000001").unwrap());
    }
}
