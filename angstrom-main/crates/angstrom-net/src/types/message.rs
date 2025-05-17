#![allow(missing_docs)]
use std::{fmt::Debug, sync::Arc};

use alloy::{
    primitives::Bytes,
    rlp::{Buf, BufMut, Decodable, Encodable}
};
use angstrom_types::{
    consensus::{PreProposal, PreProposalAggregation, Proposal},
    orders::CancelOrderRequest,
    sol_bindings::grouped_orders::AllOrders
};
use reth_eth_wire::{Capability, protocol::Protocol};
use reth_network_p2p::error::RequestError;
use serde::{Deserialize, Serialize};

use crate::errors::StromStreamError;
/// Result alias for result of a request.
pub type RequestResult<T> = Result<T, RequestError>;
use crate::Status;

/// [`MAX_MESSAGE_SIZE`] is the maximum cap on the size of a protocol message.
// https://github.com/ethereum/go-ethereum/blob/30602163d5d8321fbc68afdcbbaf2362b2641bde/eth/protocols/eth/protocol.go#L50
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

const STROM_CAPABILITY: Capability = Capability::new_static("strom", 1);
const STROM_PROTOCOL: Protocol = Protocol::new(STROM_CAPABILITY, 5);
/// Represents message IDs for eth protocol messages.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StromMessageID {
    Status            = 0,
    /// Consensus
    PrePropose        = 1,
    PreProposeAgg     = 2,
    Propose           = 3,
    BundleUnlockAttestation = 4,
    /// Propagation messages that broadcast new orders to all peers
    PropagatePooledOrders = 5,
    OrderCancellation = 6
}

impl Encodable for StromMessageID {
    fn encode(&self, out: &mut dyn BufMut) {
        out.put_u8(*self as u8);
    }

    fn length(&self) -> usize {
        1
    }
}

impl Decodable for StromMessageID {
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy::rlp::Error> {
        let id = buf.first().ok_or(alloy::rlp::Error::InputTooShort)?;
        let id = match id {
            0 => StromMessageID::Status,
            1 => StromMessageID::PrePropose,
            2 => StromMessageID::PreProposeAgg,
            3 => StromMessageID::PrePropose,
            4 => StromMessageID::BundleUnlockAttestation,
            5 => StromMessageID::PropagatePooledOrders,
            6 => StromMessageID::OrderCancellation,
            _ => return Err(alloy::rlp::Error::Custom("Invalid message ID"))
        };
        buf.advance(1);
        Ok(id)
    }
}

/// An `eth` protocol message, containing a message ID and payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StromProtocolMessage {
    pub message_id: StromMessageID,
    pub message:    StromMessage
}

impl StromProtocolMessage {
    pub fn decode_message(buf: &mut &[u8]) -> Result<Self, StromStreamError> {
        let message_id: StromMessageID = Decodable::decode(buf)?;
        let data: Vec<u8> = Decodable::decode(buf)?;
        let message: StromMessage = bincode::deserialize(&data).unwrap();

        Ok(StromProtocolMessage { message_id, message })
    }
}

impl Encodable for StromProtocolMessage {
    fn encode(&self, out: &mut dyn BufMut) {
        Encodable::encode(&self.message_id, out);
        let buf = bincode::serialize(&self.message).unwrap();
        Encodable::encode(&buf, out);
    }
}

impl StromProtocolMessage {
    /// Returns the protocol for the `Strom` protocol.
    pub const fn protocol() -> Protocol {
        STROM_PROTOCOL
    }
}

/// Represents messages that can be sent to multiple peers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolBroadcastMessage {
    pub message_id: StromMessageID,
    pub message:    StromBroadcastMessage
}

impl From<StromBroadcastMessage> for ProtocolBroadcastMessage {
    fn from(message: StromBroadcastMessage) -> Self {
        ProtocolBroadcastMessage { message_id: message.message_id(), message }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StromMessage {
    // init
    Status(Status),
    // TODO: do we need a status ack?

    // Consensus
    PrePropose(PreProposal),
    PreProposeAgg(PreProposalAggregation),
    Propose(Proposal),
    BundleUnlockAttestation(u64, Bytes),

    // Propagation messages that broadcast new orders to all peers
    PropagatePooledOrders(Vec<AllOrders>),
    OrderCancellation(CancelOrderRequest)
}
impl StromMessage {
    /// Returns the message's ID.
    pub fn message_id(&self) -> StromMessageID {
        match self {
            StromMessage::Status(_) => StromMessageID::Status,
            StromMessage::PrePropose(_) => StromMessageID::PrePropose,
            StromMessage::PreProposeAgg(_) => StromMessageID::PreProposeAgg,
            StromMessage::Propose(_) => StromMessageID::Propose,
            StromMessage::BundleUnlockAttestation(..) => StromMessageID::BundleUnlockAttestation,
            StromMessage::PropagatePooledOrders(_) => StromMessageID::PropagatePooledOrders,
            StromMessage::OrderCancellation(_) => StromMessageID::OrderCancellation
        }
    }
}

/// Represents broadcast messages of [`StromMessage`] with the same object that
/// can be sent to multiple peers.
///
/// Messages that contain a list of hashes depend on the peer the message is
/// sent to. A peer should never receive a hash of an object (block,
/// transaction) it has already seen.
///
/// Note: This is only useful for outgoing messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StromBroadcastMessage {
    // Consensus Broadcast
    PrePropose(Arc<PreProposal>),
    Propose(Arc<Proposal>),
    PreProposeAgg(Arc<PreProposalAggregation>),
    BundleUnlockAttestation(Arc<Bytes>),
    // Order Broadcast
    PropagatePooledOrders(Arc<Vec<AllOrders>>),
    OrderCancellation(Arc<CancelOrderRequest>)
}

impl StromBroadcastMessage {
    /// Returns the message's ID.
    pub fn message_id(&self) -> StromMessageID {
        match self {
            StromBroadcastMessage::PrePropose(_) => StromMessageID::PrePropose,
            StromBroadcastMessage::PreProposeAgg(_) => StromMessageID::PreProposeAgg,
            StromBroadcastMessage::Propose(_) => StromMessageID::Propose,
            StromBroadcastMessage::BundleUnlockAttestation(_) => {
                StromMessageID::BundleUnlockAttestation
            }
            StromBroadcastMessage::PropagatePooledOrders(_) => {
                StromMessageID::PropagatePooledOrders
            }
            StromBroadcastMessage::OrderCancellation(_) => StromMessageID::OrderCancellation
        }
    }
}
