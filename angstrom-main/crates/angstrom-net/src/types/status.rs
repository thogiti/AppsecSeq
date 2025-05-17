use std::{
    fmt::{Debug, Display},
    time::{SystemTime, UNIX_EPOCH}
};

use alloy::{
    primitives::{FixedBytes, keccak256},
    rlp::{BufMut, BytesMut},
    signers::Signature
};
use angstrom_types::primitive::{AngstromSigner, PeerId};
use serde::{Deserialize, Serialize};

use crate::StatusBuilder;

/// The status message is used in the strom protocol to ensure that the
/// connecting peer is using the same protocol version and is on the same chain.
/// More crucially, it is used to verify that the connecting peer is a valid
/// staker with sufficient balance to be a validator.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state:     StatusState,
    /// the signature over all state fields concatenated
    pub signature: Signature
}

impl Status {
    /// Helper for returning a builder for the status message.
    pub fn builder(peer_id: PeerId) -> StatusBuilder {
        StatusBuilder::new(peer_id)
    }

    /// returns true if the signature is valid
    pub fn verify(self) -> Result<PeerId, alloy::signers::Error> {
        let message = self.state.to_message();
        let key = self.signature.recover_from_prehash(&message).unwrap();

        Ok(AngstromSigner::public_key_to_peer_id(&key))
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Status {{ version: {}, chain: {}}}", self.state.version, self.state.chain,)
    }
}

impl Debug for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(
                f,
                "Status {{\n\tversion: {:?},\n\tchain: {:?}}}",
                self.state.version, self.state.chain,
            )
        } else {
            write!(
                f,
                "Status {{ version: {:?}, chain: {:?}}}",
                self.state.version, self.state.chain,
            )
        }
    }
}

#[derive(Default, Copy, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusState {
    /// The current protocol version.
    pub version: u8,

    /// The chain id, as introduced in
    /// [EIP155](https://eips.ethereum.org/EIPS/eip-155#list-of-chain-ids).
    /// PROBLEM BINCODE
    pub chain:     u64,
    /// The peer that a node is trying to establish a connection with
    pub peer:      PeerId,
    /// The current timestamp. Used to make sure that the status message will
    /// expire
    pub timestamp: u128
}

impl StatusState {
    pub fn new(peer: PeerId) -> Self {
        Self { peer, ..Default::default() }
    }

    pub fn with_peer(mut self, peer: PeerId) -> Self {
        self.peer = peer;
        self
    }

    /// creates message for signing.
    /// keccak256(version || peer || timestamp)
    pub fn to_message(&self) -> FixedBytes<32> {
        let mut buf = BytesMut::with_capacity(113);
        buf.put_u8(self.version);
        buf.put_u64(self.chain);
        buf.put(self.peer.0.as_ref());
        buf.put_u128(self.timestamp);

        keccak256(buf)
    }

    pub fn timestamp_now(&mut self) {
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
    }
}
