use std::collections::HashSet;

use jsonrpsee::proc_macros::rpc;

use crate::types::GasEstimateFilter;

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "quoting"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "quoting"))]
#[async_trait::async_trait]
pub trait QuotingApi {
    #[subscription(
        name = "subscribe_gas_estimates", 
        unsubscribe = "unsubscribe_gas_estimates",
        item = crate::types::quoting::GasEstimateUpdate
    )]
    async fn subscribe_gas_estimates(
        &self,
        filters: HashSet<GasEstimateFilter>
    ) -> jsonrpsee::core::SubscriptionResult;
}
