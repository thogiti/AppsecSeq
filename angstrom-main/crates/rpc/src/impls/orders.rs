use std::collections::HashSet;

use alloy_primitives::{Address, B256, U256};
use angstrom_types::{
    orders::{CancelOrderRequest, OrderLocation, OrderOrigin, OrderStatus},
    primitive::PoolId,
    sol_bindings::{RawPoolOrder, grouped_orders::AllOrders}
};
use futures::StreamExt;
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage, core::RpcResult};
use order_pool::{OrderPoolHandle, PoolManagerUpdate};
use reth_tasks::TaskSpawner;
use validation::order::OrderValidatorHandle;

use crate::{
    api::OrderApiServer,
    types::{
        CallResult, OrderSubscriptionFilter, OrderSubscriptionKind, OrderSubscriptionResult,
        PendingOrder
    }
};

pub struct OrderApi<OrderPool, Spawner, Validator> {
    pool:         OrderPool,
    task_spawner: Spawner,
    validator:    Validator
}

impl<OrderPool, Spawner, Validator> OrderApi<OrderPool, Spawner, Validator> {
    pub fn new(pool: OrderPool, task_spawner: Spawner, validator: Validator) -> Self {
        Self { pool, task_spawner, validator }
    }
}

#[async_trait::async_trait]
impl<OrderPool, Spawner, Validator> OrderApiServer for OrderApi<OrderPool, Spawner, Validator>
where
    OrderPool: OrderPoolHandle,
    Spawner: TaskSpawner + 'static,
    Validator: OrderValidatorHandle
{
    async fn send_order(&self, order: AllOrders) -> RpcResult<CallResult> {
        match self.pool.new_order(OrderOrigin::External, order).await {
            Ok(v) => Ok(CallResult::from_success(v)),
            Err(e) => Ok(e.into())
        }
    }

    async fn pending_order(&self, from: Address) -> RpcResult<Vec<PendingOrder>> {
        Ok(self
            .pool
            .pending_orders(from)
            .await
            .into_iter()
            .map(|order| PendingOrder { order_id: order.order_hash(), order })
            .collect())
    }

    async fn cancel_order(&self, request: CancelOrderRequest) -> RpcResult<bool> {
        Ok(self.pool.cancel_order(request).await)
    }

    async fn estimate_gas(
        &self,
        is_book: bool,
        is_internal: bool,
        token_0: Address,
        token_1: Address
    ) -> RpcResult<Result<U256, String>> {
        let res = self
            .validator
            .estimate_gas(is_book, is_internal, token_0, token_1)
            .await;
        Ok(res)
    }

    async fn order_status(&self, order_hash: B256) -> RpcResult<CallResult> {
        let status = self
            .pool
            .fetch_order_status(order_hash)
            .await
            .unwrap_or(OrderStatus::OrderNotFound);
        Ok(CallResult::from_success(status))
    }

    async fn valid_nonce(&self, user: Address) -> RpcResult<u64> {
        Ok(self.validator.valid_nonce_for_user(user).await)
    }

    async fn orders_by_pool_id(
        &self,
        pool_id: PoolId,
        location: OrderLocation
    ) -> RpcResult<Vec<AllOrders>> {
        Ok(self.pool.fetch_orders_from_pool(pool_id, location).await)
    }

    async fn subscribe_orders(
        &self,
        pending: PendingSubscriptionSink,
        kind: HashSet<OrderSubscriptionKind>,
        filter: HashSet<OrderSubscriptionFilter>
    ) -> jsonrpsee::core::SubscriptionResult {
        let sink = pending.accept().await?;
        let mut subscription = self
            .pool
            .subscribe_orders()
            .map(move |update| update.map(|value| value.filter_out_order(&kind, &filter)));

        self.task_spawner.spawn(Box::pin(async move {
            while let Some(Ok(order)) = subscription.next().await {
                if sink.is_closed() {
                    break;
                }

                if let Some(result) = order {
                    match SubscriptionMessage::from_json(&result) {
                        Ok(message) => {
                            if sink.send(message).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to serialize subscription message: {:?}", e);
                        }
                    }
                }
            }
        }));

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrderApiError {
    #[error("invalid transaction signature")]
    InvalidSignature,
    #[error("failed to recover signer from signature")]
    SignatureRecoveryError,
    #[error("failed to estimate gas: {0}")]
    GasEstimationError(String)
}

impl From<OrderApiError> for jsonrpsee::types::ErrorObjectOwned {
    fn from(error: OrderApiError) -> Self {
        match error {
            OrderApiError::InvalidSignature => invalid_params_rpc_err(error.to_string()),
            OrderApiError::SignatureRecoveryError => invalid_params_rpc_err(error.to_string()),
            OrderApiError::GasEstimationError(e) => invalid_params_rpc_err(e)
        }
    }
}

pub fn invalid_params_rpc_err(msg: impl Into<String>) -> jsonrpsee::types::ErrorObjectOwned {
    rpc_err(jsonrpsee::types::error::INVALID_PARAMS_CODE, msg, None)
}

pub fn rpc_err(
    code: i32,
    msg: impl Into<String>,
    data: Option<&[u8]>
) -> jsonrpsee::types::error::ErrorObjectOwned {
    jsonrpsee::types::error::ErrorObject::owned(
        code,
        msg.into(),
        data.map(|data| {
            jsonrpsee::core::to_json_raw_value(&alloy_primitives::hex::encode_prefixed(data))
                .expect("serializing String can't fail")
        })
    )
}

trait OrderFilterMatching {
    fn filter_out_order(
        self,
        kind: &HashSet<OrderSubscriptionKind>,
        filter: &HashSet<OrderSubscriptionFilter>
    ) -> Option<OrderSubscriptionResult>;
}

impl OrderFilterMatching for PoolManagerUpdate {
    fn filter_out_order(
        self,
        kind: &HashSet<OrderSubscriptionKind>,
        filter: &HashSet<OrderSubscriptionFilter>
    ) -> Option<OrderSubscriptionResult> {
        match self {
            PoolManagerUpdate::NewOrder(order)
                if kind.contains(&OrderSubscriptionKind::NewOrders)
                    && (filter.contains(&OrderSubscriptionFilter::ByPair(order.pool_id))
                        || filter.contains(&OrderSubscriptionFilter::ByAddress(order.from()))
                        || (filter.contains(&OrderSubscriptionFilter::OnlyTOB)
                            && order.is_tob())
                        || (filter.contains(&OrderSubscriptionFilter::OnlyBook)
                            && !order.is_tob())
                        || filter.contains(&OrderSubscriptionFilter::None)) =>
            {
                Some(OrderSubscriptionResult::NewOrder(order.order))
            }
            PoolManagerUpdate::FilledOrder(block, order)
                if kind.contains(&OrderSubscriptionKind::FilledOrders)
                    && (filter.contains(&OrderSubscriptionFilter::ByPair(order.pool_id))
                        || filter.contains(&OrderSubscriptionFilter::ByAddress(order.from()))
                        || (filter.contains(&OrderSubscriptionFilter::OnlyTOB)
                            && order.is_tob())
                        || (filter.contains(&OrderSubscriptionFilter::OnlyBook)
                            && !order.is_tob())
                        || filter.contains(&OrderSubscriptionFilter::None)) =>
            {
                Some(OrderSubscriptionResult::FilledOrder(block, order.order))
            }
            PoolManagerUpdate::UnfilledOrders(order)
                if kind.contains(&OrderSubscriptionKind::UnfilledOrders)
                    && (filter.contains(&OrderSubscriptionFilter::ByPair(order.pool_id))
                        || filter.contains(&OrderSubscriptionFilter::ByAddress(order.from()))
                        || (filter.contains(&OrderSubscriptionFilter::OnlyTOB)
                            && order.is_tob())
                        || (filter.contains(&OrderSubscriptionFilter::OnlyBook)
                            && !order.is_tob())
                        || filter.contains(&OrderSubscriptionFilter::None)) =>
            {
                Some(OrderSubscriptionResult::UnfilledOrder(order.order))
            }
            PoolManagerUpdate::CancelledOrder { is_tob, order_hash, user, pool_id }
                if kind.contains(&OrderSubscriptionKind::CancelledOrders)
                    && (filter.contains(&OrderSubscriptionFilter::ByPair(pool_id))
                        || filter.contains(&OrderSubscriptionFilter::ByAddress(user))
                        || (filter.contains(&OrderSubscriptionFilter::OnlyTOB) && is_tob)
                        || (filter.contains(&OrderSubscriptionFilter::OnlyBook) && !is_tob)
                        || filter.contains(&OrderSubscriptionFilter::None)) =>
            {
                Some(OrderSubscriptionResult::CancelledOrder(order_hash))
            }
            PoolManagerUpdate::ExpiredOrder(order_hash)
                if kind.contains(&OrderSubscriptionKind::ExpiredOrders) =>
            {
                Some(OrderSubscriptionResult::ExpiredOrder(order_hash))
            }
            _ => None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{future, future::Future};

    use alloy_primitives::{Address, B256, FixedBytes, U256};
    use angstrom_network::pool_manager::OrderCommand;
    use angstrom_types::{
        orders::{OrderOrigin, OrderStatus},
        primitive::OrderValidationError,
        sol_bindings::grouped_orders::AllOrders
    };
    use futures::FutureExt;
    use order_pool::PoolManagerUpdate;
    use reth_tasks::TokioTaskExecutor;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio_stream::wrappers::BroadcastStream;
    use validation::order::{GasEstimationFuture, ValidationFuture};

    use super::*;

    // Test fixtures
    fn create_standing_order() -> AllOrders {
        AllOrders::PartialStanding(Default::default())
    }

    fn create_flash_order() -> AllOrders {
        AllOrders::ExactFlash(Default::default())
    }

    fn create_tob_order() -> AllOrders {
        AllOrders::TOB(Default::default())
    }

    #[tokio::test]
    async fn test_send_order() {
        let (_handle, api) = setup_order_api();

        // Test standing order
        let standing_order = create_standing_order();
        assert!(
            api.send_order(standing_order)
                .await
                .expect("to not throw error")
                .is_ok()
        );

        // Test flash order
        let flash_order = create_flash_order();
        assert!(
            api.send_order(flash_order)
                .await
                .expect("to not throw error")
                .is_ok()
        );

        // Test TOB order
        let tob_order = create_tob_order();
        assert!(
            api.send_order(tob_order)
                .await
                .expect("to not throw error")
                .is_ok()
        );
    }

    fn setup_order_api()
    -> (OrderApiTestHandle, OrderApi<MockOrderPoolHandle, TokioTaskExecutor, MockValidator>) {
        let (to_pool, pool_rx) = unbounded_channel();
        let pool_handle = MockOrderPoolHandle::new(to_pool);
        let task_executor = TokioTaskExecutor::default();
        let api = OrderApi::new(pool_handle.clone(), task_executor, MockValidator);
        let handle = OrderApiTestHandle { _from_api: pool_rx };
        (handle, api)
    }

    struct OrderApiTestHandle {
        _from_api: UnboundedReceiver<OrderCommand>
    }

    #[derive(Clone)]
    struct MockOrderPoolHandle {
        sender: UnboundedSender<OrderCommand>
    }

    impl MockOrderPoolHandle {
        fn new(sender: UnboundedSender<OrderCommand>) -> Self {
            Self { sender }
        }
    }

    impl OrderPoolHandle for MockOrderPoolHandle {
        fn fetch_orders_from_pool(
            &self,
            _: PoolId,
            _: OrderLocation
        ) -> impl Future<Output = Vec<AllOrders>> + Send {
            future::ready(vec![])
        }

        fn new_order(
            &self,
            origin: OrderOrigin,
            order: AllOrders
        ) -> impl Future<Output = Result<FixedBytes<32>, OrderValidationError>> + Send {
            let (tx, _) = tokio::sync::oneshot::channel();
            let _ = self
                .sender
                .send(OrderCommand::NewOrder(origin, order, tx))
                .is_ok();
            future::ready(Ok(FixedBytes::<32>::default()))
        }

        fn subscribe_orders(&self) -> BroadcastStream<PoolManagerUpdate> {
            unimplemented!("Not needed for this test")
        }

        fn cancel_order(&self, req: CancelOrderRequest) -> impl Future<Output = bool> + Send {
            let (tx, _) = tokio::sync::oneshot::channel();
            let _ = self.sender.send(OrderCommand::CancelOrder(req, tx)).is_ok();
            future::ready(true)
        }

        fn pending_orders(&self, address: Address) -> impl Future<Output = Vec<AllOrders>> + Send {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = self
                .sender
                .send(OrderCommand::PendingOrders(address, tx))
                .is_ok();
            rx.map(|res| res.unwrap_or_default())
        }

        fn fetch_order_status(&self, _: B256) -> impl Future<Output = Option<OrderStatus>> + Send {
            future::ready(None)
        }
    }

    #[derive(Debug, Clone)]
    struct MockValidator;

    impl OrderValidatorHandle for MockValidator {
        type Order = AllOrders;

        fn validate_order(&self, _origin: OrderOrigin, _order: Self::Order) -> ValidationFuture {
            unimplemented!("order validation is complicated")
        }

        fn new_block(
            &self,
            _block_number: u64,
            _completed_orders: Vec<B256>,
            _addresses: Vec<Address>
        ) -> ValidationFuture {
            unimplemented!("no new block")
        }

        fn estimate_gas(
            &self,
            _is_book: bool,
            _is_internal: bool,
            _token_0: Address,
            _token_1: Address
        ) -> GasEstimationFuture {
            Box::pin(future::ready(Ok(U256::from(250_000u64))))
        }

        fn valid_nonce_for_user(&self, _address: Address) -> validation::order::NonceFuture {
            Box::pin(async move { 50 })
        }

        fn cancel_order(&self, _: Address, _: B256) {}
    }
}
