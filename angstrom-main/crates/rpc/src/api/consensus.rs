use std::collections::HashSet;

use alloy_primitives::Address;
use consensus::{AngstromValidator, ConsensusDataWithBlock};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "consensus"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "consensus"))]
#[async_trait::async_trait]
pub trait ConsensusApi {
    #[subscription(
        name = "subscribeEmptyBlockAttestations",
        unsubscribe = "unsubscribeEmptyBlockAttestations",
        item = ConsensusDataWithBlock<alloy_primitives::Bytes>
    )]
    async fn subscribe_empty_block_attestations(&self) -> jsonrpsee::core::SubscriptionResult;

    #[method(name = "getCurrentLeader")]
    async fn get_current_leader(&self) -> RpcResult<ConsensusDataWithBlock<Address>>;

    #[method(name = "fetchConsensusState")]
    async fn fetch_consensus_state(
        &self
    ) -> RpcResult<ConsensusDataWithBlock<HashSet<AngstromValidator>>>;
}
