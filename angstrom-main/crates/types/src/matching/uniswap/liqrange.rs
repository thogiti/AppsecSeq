use std::{fmt::Debug, ops::Deref};

use eyre::eyre;
use serde::{Deserialize, Serialize};
use uniswap_v3_math::tick_math::{MAX_TICK, MIN_TICK};

use super::{Direction, PoolPrice, PoolSnapshot, Tick};
use crate::matching::SqrtPriceX96;

/// A LiqRange describes the liquidity conditions within a specific range of
/// ticks.  A LiqRange covers ticks [lower_tick, upper_tick)
#[derive(Default, Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
pub struct LiqRange {
    /// Lower tick for this range
    pub(super) lower_tick:     Tick,
    /// Upper tick for this range
    pub(super) upper_tick:     Tick,
    /// Total liquidity within this range
    pub(super) liquidity:      u128,
    pub(super) is_tick_edge:   bool,
    pub(super) is_initialized: bool,
    pub(super) fee:            u32,
    /// The direction in which this range is valid for.
    /// this is because ranges change based on which way you are
    /// swapping through the pool. direction is zero for 1 or
    /// true == ask
    pub(super) direction:      bool
}

impl LiqRange {
    pub fn new_init(
        lower_tick: Tick,
        upper_tick: Tick,
        liquidity: u128,
        fee: u32,
        direction: bool
    ) -> eyre::Result<Self> {
        // Validate our inputs
        if upper_tick <= lower_tick {
            return Err(eyre!(
                "Upper tick bound less than or equal to lower tick bound for range ({}, {})",
                lower_tick,
                upper_tick
            ));
        }
        if upper_tick > MAX_TICK {
            return Err(eyre!("Proposed upper tick '{}' out of valid tick range", upper_tick));
        }
        if lower_tick < MIN_TICK {
            return Err(eyre!("Proposed lower tick '{}' out of valid tick range", lower_tick));
        }
        Ok(Self {
            lower_tick,
            upper_tick,
            liquidity,
            is_tick_edge: false,
            is_initialized: true,
            fee,
            direction
        })
    }

    pub fn new_uninit(
        lower_tick: Tick,
        upper_tick: Tick,
        liquidity: u128,
        is_tick_edge: bool,
        is_initialized: bool,
        fee: u32,
        direction: bool
    ) -> eyre::Result<Self> {
        // Validate our inputs
        if upper_tick <= lower_tick {
            return Err(eyre!(
                "Upper tick bound less than or equal to lower tick bound for range ({}, {})",
                lower_tick,
                upper_tick
            ));
        }
        if upper_tick > MAX_TICK {
            return Err(eyre!("Proposed upper tick '{}' out of valid tick range", upper_tick));
        }
        if lower_tick < MIN_TICK {
            return Err(eyre!("Proposed lower tick '{}' out of valid tick range", lower_tick));
        }
        Ok(Self { lower_tick, upper_tick, liquidity, is_tick_edge, is_initialized, fee, direction })
    }

    pub fn lower_tick(&self) -> i32 {
        self.lower_tick
    }

    pub fn upper_tick(&self) -> i32 {
        self.upper_tick
    }

    pub fn liquidity(&self) -> u128 {
        self.liquidity
    }
}

#[derive(Copy, Clone)]
pub struct LiqRangeRef<'a> {
    pub(super) pool_snap: &'a PoolSnapshot,
    pub(super) range:     &'a LiqRange,
    pub(super) range_idx: usize
}

impl Deref for LiqRangeRef<'_> {
    type Target = LiqRange;

    fn deref(&self) -> &Self::Target {
        self.range
    }
}

impl PartialEq for LiqRangeRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.pool_snap, other.pool_snap)
            && std::ptr::eq(self.range, other.range)
            && self.range_idx == other.range_idx
    }
}

impl Eq for LiqRangeRef<'_> {}

impl<'a> LiqRangeRef<'a> {
    pub fn new(market: &'a PoolSnapshot, range: &'a LiqRange, range_idx: usize) -> Self {
        Self { pool_snap: market, range, range_idx }
    }

    /// Determines if a given SqrtPriceX96 is within this liquidity range
    pub fn price_in_range(&self, price: SqrtPriceX96) -> bool {
        if let Ok(price_tick) = price.to_tick() {
            price_tick >= self.lower_tick && price_tick < self.upper_tick
        } else {
            false
        }
    }

    pub fn start_tick(&self, direction: Direction) -> Tick {
        match direction {
            Direction::BuyingT0 => self.lower_tick,
            Direction::SellingT0 => self.upper_tick
        }
    }

    /// Returns the final tick in this liquidity range presuming the price
    /// starts
    pub fn end_tick(&self, direction: Direction) -> Tick {
        match direction {
            Direction::BuyingT0 => self.upper_tick,
            Direction::SellingT0 => self.lower_tick
        }
    }

    /// PoolPrice representing the start price of this liquidity bound
    pub fn start_price(&self, direction: Direction) -> PoolPrice<'a> {
        let tick = self.start_tick(direction);
        PoolPrice {
            tick,
            liq_range: *self,
            price: SqrtPriceX96::at_tick(tick).unwrap(),
            fee: self.fee,
            direction: !direction.is_bid()
        }
    }

    /// PoolPrice representing the end price of this liquidity bound
    pub fn end_price(&self, direction: Direction) -> PoolPrice<'a> {
        let tick = self.end_tick(direction);
        PoolPrice {
            tick,
            liq_range: *self,
            price: SqrtPriceX96::at_tick(tick).unwrap(),
            fee: self.fee,
            direction: !direction.is_bid()
        }
    }

    /// Returns the appropriate tick to donate to in order to reward LPs in this
    /// position
    pub fn donate_tick(&self) -> Tick {
        self.lower_tick
    }

    /// finds the next initialized or edge tick. this is in order
    /// to properly mirror the uniswap swap logic
    pub fn next(&self, direction: Direction) -> Option<Self> {
        match direction {
            // bid, price goes up
            Direction::BuyingT0 => self
                .pool_snap
                .get_range_for_tick(self.range.upper_tick, false),
            // ask price goes down
            Direction::SellingT0 => self
                .pool_snap
                .get_range_for_tick(self.range.lower_tick - 1, true)
        }
    }
}

impl Debug for LiqRangeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_struct("LiqRangeRef");
        builder.field("range", &self.range);
        builder.field("range_idx", &self.range_idx);
        builder.finish()
    }
}
