use std::{fmt::Debug, sync::Arc};

use alloy::primitives::{Address, B256, U256, hex, keccak256};
use reth_revm::DatabaseRef;

/// The nonce location for quick db lookup
const ANGSTROM_NONCE_SLOT_CONST: [u8; 4] = hex!("daa050e9");

#[derive(Clone)]
pub struct Nonces(Address);

impl Nonces {
    pub fn new(angstrom_address: Address) -> Self {
        Self(angstrom_address)
    }

    pub fn get_nonce_word_slot(&self, user: Address, nonce: u64) -> B256 {
        let nonce = nonce.to_be_bytes();
        let mut arry = [0u8; 31];
        arry[0..20].copy_from_slice(&**user);
        arry[20..24].copy_from_slice(&ANGSTROM_NONCE_SLOT_CONST);
        arry[24..31].copy_from_slice(&nonce[0..7]);
        keccak256(arry)
    }

    pub fn is_valid_nonce<DB: revm::DatabaseRef>(
        &self,
        user: Address,
        nonce: u64,
        db: Arc<DB>
    ) -> bool
    where
        <DB as DatabaseRef>::Error: Sync + Send + 'static + Debug
    {
        let slot = self.get_nonce_word_slot(user, nonce);

        let word = db.storage_ref(self.0, slot.into()).unwrap();
        tracing::debug!(?word);
        let flag = U256::from(1) << (nonce as u8);

        let out = (word ^ flag) & flag == flag;
        tracing::debug!(?word, %out);
        out
    }
}
