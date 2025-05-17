use alloy_primitives::{
    Address,
    aliases::{I24, U24}
};
use angstrom_types::{
    contract_bindings::angstrom::Angstrom::PoolKey,
    matching::{
        SqrtPriceX96,
        uniswap::{LiqRange, PoolPrice, PoolSnapshot}
    },
    primitive::PoolId,
    sol_bindings::grouped_orders::{GroupedVanillaOrder, OrderWithStorageData}
};

#[derive(Debug)]
pub enum OrderType {
    FixedOrders(Vec<OrderWithStorageData<GroupedVanillaOrder>>),
    GeneratedOrders(usize)
}

impl Default for OrderType {
    fn default() -> Self {
        Self::FixedOrders(vec![])
    }
}

#[derive(Clone, Debug)]
pub struct Pool {
    key:      PoolKey,
    snapshot: PoolSnapshot,
    tob:      Address
}

impl Pool {
    pub fn new(key: PoolKey, snapshot: PoolSnapshot, tob: Address) -> Self {
        Self { key, snapshot, tob }
    }

    pub fn price(&self) -> PoolPrice {
        self.snapshot.current_price(true)
    }

    pub fn id(&self) -> PoolId {
        self.key.clone().into()
    }

    pub fn tob_recipient(&self) -> Address {
        self.tob
    }

    pub fn token0(&self) -> Address {
        self.key.currency0
    }

    pub fn token1(&self) -> Address {
        self.key.currency1
    }
}

#[derive(Default, Debug)]
pub struct PoolBuilder {
    key: Option<PoolKey>,
    amm: Option<PoolSnapshot>,
    tob: Option<Address>
}

impl PoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn random_key() -> PoolKey {
        PoolKey {
            currency0:   Address::random(),
            currency1:   Address::random(),
            fee:         U24::ZERO,
            tickSpacing: I24::unchecked_from(10),
            hooks:       Address::default()
        }
    }

    fn random_snapshot() -> PoolSnapshot {
        let price = SqrtPriceX96::at_tick(100000).unwrap();
        let ranges = vec![
            LiqRange::new_init(99000, 101000, 1_000_000_000_000_000_000_u128, 0, true).unwrap(),
            LiqRange::new_init(99000, 101000, 1_000_000_000_000_000_000_u128, 0, false).unwrap(),
        ];
        PoolSnapshot::new(10, ranges, price, 0).unwrap()
    }

    pub fn with_key(self, key: PoolKey) -> Self {
        Self { key: Some(key), ..self }
    }

    pub fn tob(self, tob: Address) -> Self {
        Self { tob: Some(tob), ..self }
    }

    pub fn snapshot(self, snapshot: PoolSnapshot) -> Self {
        Self { amm: Some(snapshot), ..self }
    }

    pub fn build(self) -> Pool {
        let key = self.key.unwrap_or_else(Self::random_key);
        let snapshot = self.amm.unwrap_or_else(Self::random_snapshot);
        let tob = self.tob.unwrap_or_else(|| Address::random());
        Pool { key, snapshot, tob }
    }
}

pub fn create_key(token0: Address, token1: Address, tick_spacing: i32) -> PoolKey {
    let (currency0, currency1) = if token0 < token1 { (token0, token1) } else { (token1, token0) };
    PoolKey {
        currency0,
        currency1,
        fee: U24::ZERO,
        tickSpacing: I24::unchecked_from(tick_spacing),
        hooks: Address::random()
    }
}
