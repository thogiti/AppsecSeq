use serde::{Deserialize, Serialize};

use super::OrderVolume;

#[derive(Copy, Clone, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderFillState {
    /// The order has not yet been processed
    #[default]
    Unfilled,
    /// The order has been completely filled
    CompleteFill,
    /// The order has been partially filled (and how much)
    PartialFill(OrderVolume),
    /// We have dropped this order, it can not or should not be filled.
    Killed
}

impl OrderFillState {
    pub fn is_filled(&self) -> bool {
        matches!(self, Self::CompleteFill | Self::PartialFill(_))
    }

    pub fn partial_q(&self) -> Option<OrderVolume> {
        match self {
            Self::PartialFill(v) => Some(*v),
            _ => None
        }
    }

    pub fn partial_fill(&self, quantity: OrderVolume) -> Self {
        match self {
            Self::Unfilled => Self::PartialFill(quantity),
            Self::PartialFill(f) => Self::PartialFill(f + quantity),
            Self::CompleteFill | Self::Killed => *self
        }
    }
}
