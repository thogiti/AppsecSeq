use std::{collections::HashMap, future::Future, sync::Arc};

use alloy::{
    primitives::{Address, BlockNumber, U256, aliases::I24},
    providers::Provider,
    sol,
    sol_types::{SolEvent, SolType}
};
use alloy_primitives::{B256, I256, Log};
use angstrom_types::{
    matching::{Ray, SqrtPriceX96},
    primitive::{PoolId as AngstromPoolId, UniswapPoolRegistry}
};
use itertools::Itertools;
use uniswap_v3_math::tick_math::{MAX_TICK, MIN_TICK};

use super::loaders::{
    get_uniswap_v_4_pool_data::GetUniswapV4PoolData,
    get_uniswap_v_4_tick_data::GetUniswapV4TickData
};
use crate::uniswap::{i128_to_i256, pool::PoolError};

sol! {
    #[derive(Debug)]
    struct PoolData {
        address tokenA;
        uint8 tokenADecimals;
        address tokenB;
        uint8 tokenBDecimals;
        uint128 liquidity;
        uint160 sqrtPrice;
        int24 tick;
        int24 tickSpacing;
        uint24 fee;
        int128 liquidityNet;
    }

    struct PoolDataV4 {
        uint8 token0Decimals;
        uint8 token1Decimals;
        uint128 liquidity;
        uint160 sqrtPrice;
        int24 tick;
        int128 liquidityNet;
    }

    #[derive(Debug, Hash, PartialEq, Eq, Copy, Default)]
    struct TickData {
        bool initialized;
        int24 tick;
        uint128 liquidityGross;
        int128 liquidityNet;
    }

    #[derive(Debug)]
    struct TicksWithBlock {
        TickData[] ticks;
        uint256 validTo;
        uint256 blockNumber;
    }
}

impl TickData {
    pub fn default_lowest() -> Self {
        Self { tick: I24::unchecked_from(MIN_TICK), ..Default::default() }
    }

    pub fn default_highest() -> Self {
        Self { tick: I24::unchecked_from(MAX_TICK), ..Default::default() }
    }
}

impl PoolData {
    pub fn get_raw_price(&self) -> Ray {
        Ray::from(SqrtPriceX96::from(self.sqrtPrice))
    }
}

sol! {
    type PoolId is bytes32;

    #[derive(Debug, PartialEq, Eq)]
    contract IUniswapV4Pool {
        event Swap(PoolId indexed id, address indexed sender, int128 amount0, int128 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick, uint24 fee);
        event ModifyLiquidity(PoolId indexed id, address indexed sender, int24 tickLower, int24 tickUpper, int256 liquidityDelta, bytes32 salt);
    }

    #[derive(Debug, PartialEq, Eq)]
    contract IUniswapV3Pool {
        event Swap(address indexed sender, address indexed recipient, int256 amount0, int256 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick);
        event Burn(address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1);
        event Mint(address sender, address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1);
    }
}

#[derive(Debug, Clone)]
pub struct SwapEvent {
    pub sender:         Address,
    pub amount0:        I256,
    pub amount1:        I256,
    pub sqrt_price_x96: U256,
    pub liquidity:      u128,
    pub tick:           i32
}

#[derive(Debug, Clone)]
pub struct ModifyPositionEvent {
    pub sender:          Address,
    pub tick_lower:      i32,
    pub tick_upper:      i32,
    pub liquidity_delta: i128
}

#[derive(Debug, Default, Clone)]
pub struct DataLoader {
    private_address: AngstromPoolId,
    public_address:  AngstromPoolId,
    pool_registry:   Option<UniswapPoolRegistry>,
    pool_manager:    Option<Address>
}

impl DataLoader {
    pub fn pool_registry(&self) -> Option<UniswapPoolRegistry> {
        self.pool_registry.clone()
    }

    pub fn pool_manager_opt(&self) -> Option<Address> {
        self.pool_manager
    }
}

pub trait PoolDataLoader: Clone {
    fn load_tick_data<P: Provider>(
        &self,
        current_tick: I24,
        zero_for_one: bool,
        num_ticks: u16,
        tick_spacing: I24,
        block_number: Option<BlockNumber>,
        provider: Arc<P>
    ) -> impl Future<Output = Result<(Vec<TickData>, U256), PoolError>> + Send;

    fn load_pool_data<P: Provider>(
        &self,
        block_number: Option<BlockNumber>,
        provider: Arc<P>
    ) -> impl Future<Output = Result<PoolData, PoolError>> + Send;

    fn private_address(&self) -> AngstromPoolId;
    fn public_address(&self) -> AngstromPoolId;
    fn pool_fee(&self) -> u32;

    fn group_logs(logs: Vec<Log>) -> HashMap<AngstromPoolId, Vec<Log>>;
    fn event_signatures() -> Vec<B256>;
    fn is_swap_event(log: &Log) -> bool;
    fn is_modify_position_event(log: &Log) -> bool;
    fn decode_swap_event(log: &Log) -> Result<SwapEvent, PoolError>;
}

impl DataLoader {
    fn pool_manager(&self) -> Address {
        self.pool_manager
            .expect("pool_manager must be set for V4 pools")
    }

    pub fn new_with_registry(
        private_address: AngstromPoolId,
        public_address: AngstromPoolId,
        registry: UniswapPoolRegistry,
        pool_manager: Address
    ) -> Self {
        Self {
            private_address,
            public_address,
            pool_registry: Some(registry),
            pool_manager: Some(pool_manager)
        }
    }
}

impl PoolDataLoader for DataLoader {
    fn pool_fee(&self) -> u32 {
        let id = self.public_address();

        let pool_key = self
            .pool_registry
            .as_ref()
            .unwrap()
            .get(&id)
            .unwrap()
            .clone();

        pool_key.fee.to()
    }

    async fn load_pool_data<P: Provider>(
        &self,
        block_number: Option<BlockNumber>,
        provider: Arc<P>
    ) -> Result<PoolData, PoolError> {
        let id = self.public_address();

        let pool_key = self
            .pool_registry
            .as_ref()
            .unwrap()
            .get(&id)
            .unwrap()
            .clone();

        tracing::trace!(?block_number, ?pool_key, "loading pool data");

        let deployer = GetUniswapV4PoolData::deploy_builder(
            provider,
            self.private_address(),
            self.pool_manager(),
            pool_key.currency0,
            pool_key.currency1
        );

        let data = match block_number {
            Some(number) => deployer.call_raw().block(number.into()).await?,
            None => deployer.call_raw().await?
        };

        let pool_data_v4 = PoolDataV4::abi_decode(&data)?;

        Ok(PoolData {
            tokenA:         pool_key.currency0,
            tokenADecimals: pool_data_v4.token0Decimals,
            tokenB:         pool_key.currency1,
            tokenBDecimals: pool_data_v4.token1Decimals,
            liquidity:      pool_data_v4.liquidity,
            sqrtPrice:      pool_data_v4.sqrtPrice,
            tick:           pool_data_v4.tick,
            tickSpacing:    pool_key.tickSpacing,
            fee:            pool_key.fee,
            liquidityNet:   pool_data_v4.liquidityNet
        })
    }

    async fn load_tick_data<P: Provider>(
        &self,
        current_tick: I24,
        zero_for_one: bool,
        num_ticks: u16,
        tick_spacing: I24,
        block_number: Option<BlockNumber>,
        provider: Arc<P>
    ) -> Result<(Vec<TickData>, U256), PoolError> {
        let deployer = GetUniswapV4TickData::deploy_builder(
            provider.clone(),
            self.private_address(),
            self.pool_manager(),
            zero_for_one,
            current_tick,
            num_ticks,
            tick_spacing
        );

        let data = match block_number {
            Some(number) => deployer.call_raw().block(number.into()).await?,
            None => deployer.call_raw().await?
        };

        let result = TicksWithBlock::abi_decode(&data)?;

        Ok((
            result
                .ticks
                .into_iter()
                .take(result.validTo.to::<usize>())
                .collect::<Vec<_>>(),
            result.blockNumber
        ))
    }

    fn public_address(&self) -> AngstromPoolId {
        self.public_address
    }

    fn private_address(&self) -> AngstromPoolId {
        self.private_address
    }

    fn group_logs(logs: Vec<Log>) -> HashMap<AngstromPoolId, Vec<Log>> {
        logs.into_iter()
            .filter_map(|log| {
                if Self::is_modify_position_event(&log) {
                    let modify_event = IUniswapV4Pool::ModifyLiquidity::decode_log(&log).ok()?;
                    return Some((modify_event.id, log));
                } else if Self::is_swap_event(&log) {
                    let swap = IUniswapV4Pool::Swap::decode_log(&log).ok()?;
                    return Some((swap.id, log));
                };
                None
            })
            .into_group_map()
    }

    fn event_signatures() -> Vec<B256> {
        vec![IUniswapV4Pool::Swap::SIGNATURE_HASH, IUniswapV4Pool::ModifyLiquidity::SIGNATURE_HASH]
    }

    fn is_swap_event(log: &Log) -> bool {
        log.topics()
            .iter()
            .any(|t| *t == IUniswapV4Pool::Swap::SIGNATURE_HASH)
    }

    fn is_modify_position_event(log: &Log) -> bool {
        log.topics()
            .iter()
            .any(|t| *t == IUniswapV4Pool::ModifyLiquidity::SIGNATURE_HASH)
    }

    fn decode_swap_event(log: &Log) -> Result<SwapEvent, PoolError> {
        let swap_event = IUniswapV4Pool::Swap::decode_log(log)?;
        Ok(SwapEvent {
            sender:         swap_event.sender,
            amount0:        i128_to_i256(swap_event.amount0),
            amount1:        i128_to_i256(swap_event.amount1),
            sqrt_price_x96: U256::from(swap_event.sqrtPriceX96),
            liquidity:      swap_event.liquidity,
            tick:           swap_event.tick.as_i32()
        })
    }
}
