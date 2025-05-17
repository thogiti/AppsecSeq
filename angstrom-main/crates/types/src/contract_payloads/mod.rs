use alloy::{
    primitives::{Address, Bytes, FixedBytes, U256},
    sol
};
use alloy_primitives::{B256, normalize_v};
use pade_macro::{PadeDecode, PadeEncode};
use serde::{Deserialize, Serialize};

pub mod angstrom;
pub mod asset;
pub mod rewards;

pub const CONFIG_STORE_SLOT: u32 = 3;
pub const POOL_CONFIG_STORE_ENTRY_SIZE: usize = 32;

sol! {
    #[derive(Debug, Default, PartialEq, PadeEncode, PadeDecode, Eq,Ord, PartialOrd, Serialize, Deserialize)]
    struct Asset {
        address addr;
        uint128 save;
        uint128 take;
        uint128 settle;
    }

    #[derive(Debug, Default, PartialEq, PadeEncode, PadeDecode,Eq, Ord, PartialOrd, Serialize, Deserialize)]
    struct Pair {
        uint16 index0;
        uint16 index1;
        uint16 store_index;
        uint256 price_1over0;
    }
}

#[derive(
    Clone,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PadeEncode,
    PadeDecode,
    Serialize,
    Deserialize,
    Ord,
    PartialOrd,
)]
pub enum Signature {
    Contract { from: Address, signature: Bytes },
    Ecdsa { v: u8, r: FixedBytes<32>, s: FixedBytes<32> }
}

impl Signature {
    pub fn recover_signer(&self, hash: B256) -> Address {
        match self {
            Self::Contract { from, .. } => *from,
            Self::Ecdsa { v, r, s } => {
                let sig = alloy::primitives::Signature::new(
                    U256::from_be_slice(&**r),
                    U256::from_be_slice(&**s),
                    normalize_v(*v as u64).unwrap()
                );
                sig.recover_address_from_prehash(&hash).unwrap()
            }
        }
    }
}

impl Default for Signature {
    fn default() -> Self {
        Self::Contract { from: Address::default(), signature: Bytes::default() }
    }
}

impl From<alloy::primitives::Signature> for Signature {
    fn from(value: alloy::primitives::Signature) -> Self {
        let v = 27 + value.v() as u8;
        let r: FixedBytes<32> = value.r().into();
        let s: FixedBytes<32> = value.s().into();
        Self::Ecdsa { v, r, s }
    }
}
