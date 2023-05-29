use cosmwasm_std::{CosmosMsg, StdError, StdResult, SubMsg};

use injective_cosmwasm::{InjectiveMsgWrapper, SpotMarket};
use injective_math::FPDecimal;
use num_traits::pow::Pow;

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

pub fn counter_denom<'a>(market: &'a SpotMarket, denom: &str) -> StdResult<&'a str> {
    if market.quote_denom == denom {
        Ok(&market.base_denom)
    } else if market.base_denom == denom {
        Ok(&market.quote_denom)
    } else {
        Err(StdError::generic_err(
            "Denom must be either base or quote denom of this market!",
        ))
    }
}

pub trait Scaled {
    fn scaled(self, digits: i32) -> Self;
}

impl Scaled for FPDecimal {
    fn scaled(self, digits: i32) -> Self {
        self.to_owned() * FPDecimal::from(10i128).pow(FPDecimal::from(digits as i128))
    }
}

pub fn dec_scale_factor() -> FPDecimal {
    FPDecimal::one().scaled(18)
    // FPDecimal::from(1000000000000000000_i128)
}


#[test]
fn test_descale () {
    let val  = FPDecimal::must_from_str("1000000000000000000");
    let descaled = val.scaled(-18);
    assert_eq!(descaled, FPDecimal::from(1u128));
    let scaled = descaled.scaled(18);
    assert_eq!(scaled, val);
}
