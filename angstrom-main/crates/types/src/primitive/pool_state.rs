use alloy::{
    primitives::{FixedBytes, Log},
    sol_types::SolValue
};
use alloy_primitives::{Address, keccak256};

use crate::contract_bindings::{
    angstrom::Angstrom::PoolKey, pool_manager::PoolManager::Initialize
};

pub type PoolId = FixedBytes<32>;

impl From<PoolKey> for PoolId {
    fn from(value: PoolKey) -> Self {
        keccak256(value.abi_encode())
    }
}

pub type PoolIdWithDirection = (bool, PoolId);

/// just a placeholder type so i can implement the general architecture
#[derive(Debug, Clone, Copy)]
pub struct NewInitializedPool {
    pub currency_in:  Address,
    pub currency_out: Address,
    pub id:           PoolId
}

impl From<Log<Initialize>> for NewInitializedPool {
    fn from(value: Log<Initialize>) -> Self {
        Self {
            currency_in:  value.currency0,
            currency_out: value.currency1,
            id:           value.id
        }
    }
}
