mod leader_selection;
mod manager;

pub use manager::*;
pub mod rounds;
use std::{collections::HashSet, pin::Pin};

use alloy::primitives::{Address, Bytes};
use futures::Stream;
pub use leader_selection::AngstromValidator;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::{self, channel},
    oneshot
};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConsensusDataWithBlock<T> {
    pub data:  T,
    pub block: u64
}

pub trait ConsensusHandle: Send + Sync + Clone + Unpin + 'static {
    fn subscribe_empty_block_attestations(
        &self
    ) -> Pin<Box<dyn Stream<Item = ConsensusDataWithBlock<Bytes>> + Send>>;

    fn get_current_leader(
        &self
    ) -> impl Future<Output = eyre::Result<ConsensusDataWithBlock<Address>>> + Send;

    fn fetch_consensus_state(
        &self
    ) -> impl Future<Output = eyre::Result<ConsensusDataWithBlock<HashSet<AngstromValidator>>>> + Send;
}

#[derive(Clone)]
pub struct ConsensusHandler(pub tokio::sync::mpsc::UnboundedSender<ConsensusRequest>);

impl ConsensusHandle for ConsensusHandler {
    async fn get_current_leader(&self) -> eyre::Result<ConsensusDataWithBlock<Address>> {
        let (tx, rx) = oneshot::channel();
        self.0.send(ConsensusRequest::CurrentLeader(tx))?;

        rx.await.map_err(Into::into)
    }

    async fn fetch_consensus_state(
        &self
    ) -> eyre::Result<ConsensusDataWithBlock<HashSet<AngstromValidator>>> {
        let (tx, rx) = oneshot::channel();
        self.0.send(ConsensusRequest::CurrentConsensusState(tx))?;

        rx.await.map_err(Into::into)
    }

    fn subscribe_empty_block_attestations(
        &self
    ) -> Pin<Box<dyn Stream<Item = ConsensusDataWithBlock<Bytes>> + Send>> {
        let (tx, rx) = channel(5);
        let _ = self.0.send(ConsensusRequest::SubscribeAttestations(tx));

        Box::pin(ReceiverStream::new(rx))
    }
}

pub enum ConsensusRequest {
    CurrentLeader(oneshot::Sender<ConsensusDataWithBlock<Address>>),
    CurrentConsensusState(oneshot::Sender<ConsensusDataWithBlock<HashSet<AngstromValidator>>>),
    SubscribeAttestations(mpsc::Sender<ConsensusDataWithBlock<Bytes>>)
}
