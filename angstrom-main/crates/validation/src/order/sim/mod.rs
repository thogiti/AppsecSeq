use std::{fmt::Debug, sync::Arc};

use alloy::primitives::Address;
use angstrom_metrics::validation::ValidationMetrics;
use angstrom_types::sol_bindings::{
    RawPoolOrder,
    grouped_orders::{AllOrders, OrderWithStorageData},
    rpc_orders::TopOfBlockOrder
};
use gas::OrderGasCalculations;
use revm::primitives::ruint::aliases::U256;
use tracing::error_span;

use crate::common::TokenPriceGenerator;

pub mod console_log;
mod gas;
pub use gas::{BOOK_GAS, BOOK_GAS_INTERNAL, TOB_GAS, TOB_GAS_INTERNAL};

pub type GasUsed = u64;
// needed for future use
// mod gas_inspector;

pub type GasInToken0 = U256;
/// validation relating to simulations.
#[derive(Clone)]
pub struct SimValidation<DB> {
    gas_calculator: OrderGasCalculations<DB>,
    metrics:        ValidationMetrics
}

impl<DB> SimValidation<DB>
where
    DB: Unpin + Clone + 'static + revm::DatabaseRef + reth_provider::BlockNumReader + Send + Sync,
    <DB as revm::DatabaseRef>::Error: Send + Sync + Debug
{
    pub fn new(db: Arc<DB>, angstrom_address: Address, node_address: Address) -> Self {
        let gas_calculator =
            OrderGasCalculations::new(db.clone(), Some(angstrom_address), node_address)
                .expect("failed to deploy baseline angstrom for gas calculations");
        Self { gas_calculator, metrics: ValidationMetrics::new() }
    }

    /// returns an error if we fail to convert prices or if the amount of token
    /// zero for gas is greater than the max amount specified.
    pub fn calculate_tob_gas(
        &self,
        order: &OrderWithStorageData<TopOfBlockOrder>,
        conversion: &TokenPriceGenerator,
        block: u64
    ) -> eyre::Result<(GasUsed, GasInToken0)> {
        let hash = order.order_hash();
        let user = order.from();
        let span = error_span!("tob", ?hash, ?user);
        span.in_scope(|| {
            self.metrics.fetch_gas_for_user(true, || {
                let gas_in_wei = self.gas_calculator.gas_of_tob_order(order, block)?;
                // grab order tokens;
                let (token0, token1, max_gas) = if order.asset_in < order.asset_out {
                    (order.asset_in, order.asset_out, order.max_gas_token_0())
                } else {
                    (order.asset_out, order.asset_in, order.max_gas_token_0())
                };

                let conversion_factor = conversion
                    .get_eth_conversion_price(token0, token1)
                    .ok_or_else(|| eyre::eyre!("failed to get conversion price"))?;
                let gas_token_0 = conversion_factor.inverse_quantity(gas_in_wei as u128, false);

                // convert to u256 for overflow cases.
                if gas_token_0 > max_gas {
                    eyre::bail!("gas_needed_token0 > max_gas");
                }

                Ok((gas_in_wei, U256::from(gas_token_0)))
            })
        })
    }

    /// returns an error if we fail to convert prices or if the amount of token
    /// zero for gas is greater than the max amount specified.
    pub fn calculate_user_gas(
        &self,
        order: &OrderWithStorageData<AllOrders>,
        conversion: &TokenPriceGenerator,
        block: u64
    ) -> eyre::Result<(GasUsed, GasInToken0)> {
        let hash = order.order_hash();
        let user = order.from();
        let span = error_span!("user", ?hash, ?user);
        span.in_scope(|| {
            self.metrics.fetch_gas_for_user(false, || {
                let gas_in_wei = self.gas_calculator.gas_of_book_order(order, block)?;
                // grab order tokens;
                let (token0, token1, max_gas) = if order.token_in() < order.token_out() {
                    (order.token_in(), order.token_out(), order.max_gas_token_0())
                } else {
                    (order.token_out(), order.token_in(), order.max_gas_token_0())
                };

                // grab price conversion
                let conversion_factor = conversion
                    .get_eth_conversion_price(token0, token1)
                    .ok_or_else(|| eyre::eyre!("failed to get conversion price"))?;
                let gas_token_0 = conversion_factor.inverse_quantity(gas_in_wei as u128, false);

                // convert to u256 for overflow cases.
                if gas_token_0 > max_gas {
                    eyre::bail!("gas_needed_token0 > max_gas");
                }

                Ok((gas_in_wei, U256::from(gas_token_0)))
            })
        })
    }
}
