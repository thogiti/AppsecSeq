use alloy::primitives::Address;
use tracing::{Level, event_enabled, trace};

use super::{AssetArray, state::StageTracker};
use crate::contract_payloads::Asset;

pub enum AssetBuilderStage {
    Swap,
    Reward,
    TopOfBlock,
    UserOrder
}

pub struct AssetBuilder {
    swaps:        StageTracker,
    rewards:      StageTracker,
    top_of_block: StageTracker,
    user_orders:  StageTracker,
    assets:       AssetArray
}

impl AssetBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_stage(&mut self, stage: AssetBuilderStage) -> &mut StageTracker {
        match stage {
            AssetBuilderStage::Swap => &mut self.swaps,
            AssetBuilderStage::Reward => &mut self.rewards,
            AssetBuilderStage::TopOfBlock => &mut self.top_of_block,
            AssetBuilderStage::UserOrder => &mut self.user_orders
        }
    }

    pub fn order_assets_properly(&mut self) {
        self.assets.order_assets_properly();
    }

    /// Uniswap swap, we 'take' everything from this
    pub fn uniswap_swap(
        &mut self,
        stage: AssetBuilderStage,
        asset_in: usize,
        asset_out: usize,
        quantity_in: u128,
        quantity_out: u128
    ) {
        let asset_in_addr = self.assets.get_asset_addr(asset_in);
        let asset_out_addr = self.assets.get_asset_addr(asset_out);
        tracing::info!(?asset_in_addr, ?asset_out_addr, ?quantity_in, ?quantity_out);
        self.get_stage(stage).uniswap_swap(
            asset_in_addr,
            asset_out_addr,
            quantity_in,
            quantity_out
        );
    }

    /// Uniswap swap, we 'take' everything from this. uses addresses instead of
    /// asset_ids
    pub fn uniswap_swap_raw(
        &mut self,
        stage: AssetBuilderStage,
        asset_in: Address,
        asset_out: Address,
        quantity_in: u128,
        quantity_out: u128
    ) {
        self.get_stage(stage)
            .uniswap_swap(asset_in, asset_out, quantity_in, quantity_out);
    }

    /// User swap, impacts only our own liquidity
    pub fn external_swap(
        &mut self,
        stage: AssetBuilderStage,
        asset_in: Address,
        asset_out: Address,
        quantity_in: u128,
        quantity_out: u128
    ) {
        self.get_stage(stage)
            .external_swap(asset_in, asset_out, quantity_in, quantity_out);
    }

    pub fn add_gas_fee(&mut self, stage: AssetBuilderStage, asset: Address, qty: u128) {
        self.get_stage(stage).add_gas_fee(asset, qty);
    }

    pub fn allocate(&mut self, stage: AssetBuilderStage, asset: Address, quantity: u128) {
        self.get_stage(stage).allocate(asset, quantity);
    }

    pub fn tribute(&mut self, stage: AssetBuilderStage, asset: Address, quantity: u128) {
        self.get_stage(stage).tribute(asset, quantity);
    }

    pub fn add_or_get_asset(&mut self, asset: Address) -> usize {
        self.assets.add_or_get_asset_idx(asset)
    }

    pub fn get_asset_array(&self) -> Vec<Asset> {
        if event_enabled!(target: "dump::assetbuilder", Level::TRACE) {
            trace!(target: "dump::assetbuilder", map = ?self.swaps.map, "Swap level");
            trace!(target: "dump::assetbuilder", map = ?self.rewards.map, "Reward level");
            trace!(target: "dump::assetbuilder", map = ?self.top_of_block.map, "ToB level");
            trace!(target: "dump::assetbuilder", map = ?self.user_orders.map, "User level");
        }
        let combined_assets = self
            .swaps
            .and_then(&self.rewards)
            .and_then(&self.top_of_block)
            .and_then(&self.user_orders)
            .collect_extra();
        self.assets
            .get_asset_array()
            .into_iter()
            .map(|mut asset| {
                if let Some(tracker) = combined_assets.get_asset(&asset.addr) {
                    asset.take = tracker.take;
                    asset.settle = tracker.settle;
                    asset.save = tracker.save;
                }
                asset
            })
            .collect()
    }
}

impl Default for AssetBuilder {
    fn default() -> Self {
        Self {
            swaps:        StageTracker::new(),
            rewards:      StageTracker::new(),
            top_of_block: StageTracker::new(),
            user_orders:  StageTracker::new(),
            assets:       AssetArray::new()
        }
    }
}
