use angstrom_types::{primitive::OrderValidationError, sol_bindings::RawPoolOrder};

use super::{OrderValidation, OrderValidationState};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct EnsureMaxGasLessThanMinAmount;

impl OrderValidation for EnsureMaxGasLessThanMinAmount {
    fn validate_order<O: RawPoolOrder>(
        &self,
        state: &mut OrderValidationState<O>
    ) -> Result<(), OrderValidationError> {
        if state.min_qty_in_t0() < state.order().max_gas_token_0() {
            Err(OrderValidationError::MaxGasGreaterThanMinAmount)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_max_gas_greater_than_min_error() {
        use angstrom_types::primitive::OrderValidationError;

        use crate::order::{
            AllOrders,
            state::order_validators::{
                EnsureMaxGasLessThanMinAmount, OrderValidationState, make_base_order
            }
        };

        let mut order = make_base_order();
        if let AllOrders::PartialStanding(ref mut o) = order {
            o.max_extra_fee_asset0 = o.min_amount_in + 1;
        }

        let validator = EnsureMaxGasLessThanMinAmount;
        let mut state = OrderValidationState::new(&order);
        let result = validator.validate_order(&mut state);
        assert_eq!(result, Err(OrderValidationError::MaxGasGreaterThanMinAmount));
    }
}
