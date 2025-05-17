use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod handle;
pub use handle::*;

pub mod protocol_handler;
pub use protocol_handler::*;

pub mod strom;
pub use strom::*;
pub mod config;
pub use config::*;
use futures::task::Context;
pub mod connection_handler;
use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    net::SocketAddr,
    ops::Deref,
    pin::Pin,
    sync::{Arc, atomic::AtomicU64}
};

use angstrom_types::primitive::PeerId;
pub use connection_handler::*;
use futures::{Stream, task::Poll};
use reth_eth_wire::DisconnectReason;
use reth_network::Direction;

use crate::{StromMessage, StromProtocolMessage, errors::StromStreamError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CachedPeer {
    pub peer_id: PeerId,
    pub addr:    SocketAddr
}

impl CachedPeer {
    pub const fn new(peer_id: PeerId, addr: SocketAddr) -> Self {
        Self { peer_id, addr }
    }

    pub fn enr(&self) -> String {
        format!(
            "enode://{:?}@{}:{}?discport=30303",
            self.peer_id,
            self.addr.ip(),
            30303 // self.addr.port()
        )
    }
}

#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CachedPeers {
    pub peers: Vec<CachedPeer>
}

impl From<Vec<CachedPeer>> for CachedPeers {
    fn from(peers: Vec<CachedPeer>) -> Self {
        Self { peers }
    }
}

impl CachedPeers {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Deref for CachedPeers {
    type Target = Vec<CachedPeer>;

    fn deref(&self) -> &Self::Target {
        &self.peers
    }
}

#[derive(Debug)]
pub struct StromSessionManager {
    // All active sessions that are ready to exchange messages.
    pub(crate) active_sessions: HashMap<PeerId, StromSessionHandle>,

    /// Channel to receive the session handle upon initialization from the
    /// connection handler This channel is also used to receive messages
    /// from the session
    pub(crate) from_sessions: mpsc::Receiver<StromSessionMessage>,
    msg_buf:                  VecDeque<SessionEvent>
}

impl StromSessionManager {
    pub fn new(from_sessions: mpsc::Receiver<StromSessionMessage>) -> Self {
        Self { from_sessions, active_sessions: HashMap::default(), msg_buf: VecDeque::new() }
    }

    /// Sends a message to the peer's session
    pub fn send_message(&mut self, peer_id: &PeerId, msg: StromMessage) {
        if let Some(session) = self.active_sessions.get_mut(peer_id) {
            let _ = session
                .commands_to_session
                .try_send(SessionCommand::Message(msg));
        }
    }

    pub fn broadcast_message(&mut self, msg: StromMessage) {
        self.active_sessions.values_mut().for_each(|cmd| {
            let _ = cmd
                .commands_to_session
                .try_send(SessionCommand::Message(msg.clone()));
        })
    }

    // Removes the Session handle if it exists.
    fn remove_session(&mut self, id: &PeerId) -> Option<StromSessionHandle> {
        let session = self.active_sessions.remove(id)?;
        Some(session)
    }

    /// Shutdown all active sessions.
    pub fn disconnect_all(&self, reason: Option<DisconnectReason>) {
        for (_, session) in self.active_sessions.iter() {
            session.disconnect(reason);
        }
    }

    pub fn disconnect(&mut self, id: PeerId, reason: Option<DisconnectReason>) {
        if let Some(session) = self.active_sessions.remove(&id) {
            session.disconnect(reason)
        }
    }

    fn poll_session_msg(&mut self, cx: &mut Context<'_>) -> Poll<Option<SessionEvent>> {
        // before we process any data, lets ensure we haven't had any disconnects.
        // we have to do this as we don't get any notifications of closing
        // when our objects get dropped from reth so we just check
        // to ensure all the handles channels are still open
        let dead_peers = self
            .active_sessions
            .iter()
            .filter_map(|(peer, session)| session.commands_to_session.is_closed().then_some(peer))
            .copied()
            .collect_vec();

        for dead_peer in dead_peers {
            let _ = self.active_sessions.remove(&dead_peer);
            self.msg_buf
                .push_back(SessionEvent::Disconnected { peer_id: dead_peer });
        }

        if let Some(next) = self.msg_buf.pop_front() {
            cx.waker().wake_by_ref();
            return Poll::Ready(Some(next));
        }

        self.from_sessions.poll_recv(cx).map(|msg| {
            msg.and_then(|msg_inner| match msg_inner {
                StromSessionMessage::Disconnected { peer_id } => {
                    self.remove_session(&peer_id);
                    Some(SessionEvent::Disconnected { peer_id })
                }
                StromSessionMessage::Established { handle } => {
                    if self.active_sessions.contains_key(&handle.remote_id) {
                        tracing:: warn!(peer_id=?handle.remote_id, "got duplicate connection");
                        handle.disconnect(None);
                        return None;
                    }

                    let event = SessionEvent::SessionEstablished {
                        peer_id:   handle.remote_id,
                        direction: handle.direction,
                        timeout:   Arc::new(AtomicU64::new(40))
                    };

                    // we don't need to worry about duplicate sessions as
                    // underlying reth-node handles this
                    self.active_sessions.insert(handle.remote_id, handle);
                    Some(event)
                }
                StromSessionMessage::ClosedOnConnectionError { peer_id, error } => {
                    self.remove_session(&peer_id);
                    Some(SessionEvent::OutgoingConnectionError { peer_id, error })
                }
                StromSessionMessage::ValidMessage { peer_id, message } => {
                    Some(SessionEvent::ValidMessage { peer_id, message })
                }
                StromSessionMessage::BadMessage { peer_id } => {
                    Some(SessionEvent::BadMessage { peer_id })
                }
                StromSessionMessage::ProtocolBreach { peer_id } => {
                    Some(SessionEvent::ProtocolBreach { peer_id })
                }
            })
        })
    }
}

impl Stream for StromSessionManager {
    type Item = SessionEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<SessionEvent>> {
        self.poll_session_msg(cx)
    }
}

/// Events produced by the [`SessionManager`]
#[derive(Debug)]
pub enum SessionEvent {
    /// A new session was successfully authenticated.
    ///
    /// This session is now able to exchange data.
    SessionEstablished {
        /// The remote node's public key
        peer_id:   PeerId,
        /// The direction of the session, either `Inbound` or `Outgoing`
        direction: Direction,
        /// The maximum time that the session waits for a response from the peer
        /// before timing out the connection
        timeout:   Arc<AtomicU64>
    },
    /// The peer was already connected with another session.
    AlreadyConnected {
        /// The remote node's public key
        peer_id:     PeerId,
        /// The remote node's socket address
        remote_addr: SocketAddr,
        /// The direction of the session, either `Inbound` or `Outgoing`
        direction:   Direction
    },
    /// A session received a valid message via RLPx.
    ValidMessage {
        /// The remote node's public key
        peer_id: PeerId,
        /// Message received from the peer.
        message: StromProtocolMessage
    },
    /// Received a bad message from the peer.
    BadMessage {
        /// Identifier of the remote peer.
        peer_id: PeerId
    },
    /// Remote peer is considered in protocol violation
    ProtocolBreach {
        /// Identifier of the remote peer.
        peer_id: PeerId
    },

    /// Failed to establish a tcp stream
    OutgoingConnectionError {
        /// The remote node's public key
        peer_id: PeerId,
        /// The error that caused the outgoing connection to fail
        error:   StromStreamError
    },
    /// Session was closed due to an error
    SessionClosedOnConnectionError {
        /// The id of the remote peer.
        peer_id:     PeerId,
        /// The socket we were connected to.
        remote_addr: SocketAddr,
        /// The error that caused the session to close
        error:       StromStreamError
    },
    /// Active session was gracefully disconnected.
    Disconnected {
        /// The remote node's public key
        peer_id: PeerId
    }
}
