pub mod regular;
pub mod shutdown;
pub mod startup;

use std::{fmt::Debug, pin::Pin};

use alloy::rlp::BytesMut;
use angstrom_types::primitive::{AngstromSigner, PeerId};
use futures::{
    Stream,
    task::{Context, Poll}
};
use regular::RegularProcessing;
use reth_eth_wire::multiplex::ProtocolConnection;
use reth_metrics::common::mpsc::MeteredPollSender;
use shutdown::Shutdown;
use startup::StromStartup;
use tokio_stream::wrappers::ReceiverStream;

use super::handle::SessionCommand;
use crate::{
    StatusBuilder, StromSessionHandle, StromSessionMessage,
    types::status::{Status, StatusState}
};

pub enum StromSessionStates {
    Startup(StromStartup),
    Regular(RegularProcessing),
    Shutdown(Shutdown),
    Dummy(DummyState)
}

impl Default for StromSessionStates {
    fn default() -> Self {
        Self::Dummy(DummyState)
    }
}

impl StromSession for StromSessionStates {
    fn poll_outbound_msg(&mut self, cx: &mut Context<'_>) -> Poll<Option<BytesMut>> {
        match self {
            Self::Startup(s) => s.poll_outbound_msg(cx),
            Self::Regular(s) => s.poll_outbound_msg(cx),
            Self::Shutdown(s) => s.poll_outbound_msg(cx),
            Self::Dummy(s) => s.poll_outbound_msg(cx)
        }
    }

    fn poll_next_state(self, cx: &mut Context<'_>) -> Option<StromSessionStates> {
        match self {
            Self::Startup(s) => s.poll_next_state(cx),
            Self::Regular(s) => s.poll_next_state(cx),
            Self::Shutdown(s) => s.poll_next_state(cx),
            Self::Dummy(s) => s.poll_next_state(cx)
        }
    }
}

/// this trait handles the transition and different functionality of
/// a strom session at the different points in time
pub trait StromSession: Send + 'static {
    /// Messages encoded that are meant for the peer
    fn poll_outbound_msg(&mut self, cx: &mut Context<'_>) -> Poll<Option<BytesMut>>;
    /// will transition to next state.
    fn poll_next_state(self, cx: &mut Context<'_>) -> Option<StromSessionStates>;
}

#[derive(Default)]
pub struct DummyState;
impl StromSession for DummyState {
    fn poll_outbound_msg(&mut self, _: &mut Context<'_>) -> Poll<Option<BytesMut>> {
        todo!()
    }

    fn poll_next_state(self, _: &mut Context<'_>) -> Option<StromSessionStates> {
        todo!()
    }
}

/// holds the state we need to verify the new peer
#[derive(Clone)]
pub struct VerificationSidecar {
    pub secret_key:   AngstromSigner,
    pub status:       StatusState,
    pub has_sent:     bool,
    pub has_received: bool
}

impl VerificationSidecar {
    pub fn make_status_message(&mut self, peer: PeerId) -> Status {
        if self.has_sent {
            panic!("can only send the status message once");
        }

        StatusBuilder::from(self.status.with_peer(peer)).build(&self.secret_key)
    }

    pub fn is_verified(&self) -> bool {
        self.has_sent && self.has_received
    }
}

impl Debug for VerificationSidecar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("status: {:?}", self.status))
    }
}

pub struct StromSessionHandler {
    inner_state: StromSessionStates
}

impl StromSessionHandler {
    pub fn new(
        conn: ProtocolConnection,
        peer_id: PeerId,
        commands_rx: ReceiverStream<SessionCommand>,
        to_session_manager: MeteredPollSender<StromSessionMessage>,
        verification_sidecar: VerificationSidecar,
        handle: StromSessionHandle
    ) -> Self {
        let inner_state = StromSessionStates::Startup(StromStartup::new(
            verification_sidecar,
            Some(handle),
            conn,
            peer_id,
            to_session_manager,
            commands_rx
        ));
        Self { inner_state }
    }
}

impl Stream for StromSessionHandler {
    type Item = BytesMut;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut();
        if let Poll::Ready(next) = this.inner_state.poll_outbound_msg(cx) {
            match next {
                data @ Some(_) => return Poll::Ready(data),
                // transition to next state
                None => {
                    let prev = std::mem::take(&mut this.inner_state);
                    let Some(new_state) = prev.poll_next_state(cx) else {
                        return Poll::Ready(None);
                    };

                    this.inner_state = new_state;
                }
            }
        }
        Poll::Pending
    }
}
