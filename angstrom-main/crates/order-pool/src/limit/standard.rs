use std::collections::HashMap;

use alloy::primitives::B256;
use angstrom_metrics::VanillaLimitOrderPoolMetricsWrapper;
use angstrom_types::{
    orders::{OrderId, OrderStatus},
    primitive::{NewInitializedPool, PoolId, UserAccountVerificationError},
    sol_bindings::grouped_orders::{AllOrders, OrderWithStorageData}
};
use angstrom_utils::map::OwnedMap;

use super::{parked::ParkedPool, pending::PendingPool};
use crate::limit::LimitPoolError;

#[derive(Default)]
pub struct LimitPool {
    pub(super) pending_orders: HashMap<PoolId, PendingPool<AllOrders>>,
    pub(super) parked_orders:  HashMap<PoolId, ParkedPool>,
    metrics:                   VanillaLimitOrderPoolMetricsWrapper
}

impl LimitPool {
    pub fn new(ids: &[PoolId]) -> Self {
        let parked = ids.iter().map(|id| (*id, ParkedPool::new())).collect();
        let pending = ids.iter().map(|id| (*id, PendingPool::new())).collect();

        Self {
            parked_orders:  parked,
            pending_orders: pending,
            metrics:        VanillaLimitOrderPoolMetricsWrapper::new()
        }
    }

    pub fn get_order_status(&self, order_hash: B256) -> Option<OrderStatus> {
        self.pending_orders
            .values()
            .find_map(|pool| {
                let _ = pool.get_order(order_hash)?;
                // found order return some pending
                Some(OrderStatus::Pending)
            })
            .or_else(|| {
                self.parked_orders.values().find_map(|pool| {
                    let order = pool.get_order(order_hash)?;
                    // found order return some pending
                    Some(
                        OrderStatus::try_from_err(order.is_currently_valid.as_ref().unwrap())
                            .unwrap()
                    )
                })
            })
    }

    pub fn get_order(
        &self,
        pool_id: PoolId,
        order_id: alloy::primitives::FixedBytes<32>
    ) -> Option<OrderWithStorageData<AllOrders>> {
        // Try to get from pending orders first
        self.pending_orders
            .get(&pool_id)
            .and_then(|pool| pool.get_order(order_id))
            .or_else(|| {
                // If not in pending, try parked orders
                self.parked_orders
                    .get(&pool_id)
                    .and_then(|pool| pool.get_order(order_id))
            })
    }

    pub fn add_order(
        &mut self,
        order: OrderWithStorageData<AllOrders>
    ) -> Result<(), LimitPoolError> {
        let pool_id = order.pool_id;
        let err = || LimitPoolError::NoPool(pool_id);

        if order.is_currently_valid() {
            self.pending_orders
                .get_mut(&pool_id)
                .ok_or_else(err)?
                .add_order(order);
            self.metrics.incr_pending_orders(pool_id, 1);
        } else {
            self.parked_orders
                .get_mut(&pool_id)
                .ok_or_else(err)?
                .new_order(order);
            self.metrics.incr_parked_orders(pool_id, 1);
        }

        Ok(())
    }

    pub fn remove_parked_order(
        &mut self,
        pool_id: PoolId,
        order_id: alloy::primitives::FixedBytes<32>
    ) -> Option<OrderWithStorageData<AllOrders>> {
        self.parked_orders.get_mut(&pool_id).and_then(|pool| {
            pool.remove_order(order_id)
                .owned_map(|| self.metrics.decr_parked_orders(pool_id, 1))
        })
    }

    pub fn remove_order(
        &mut self,
        pool_id: PoolId,
        order_id: alloy::primitives::FixedBytes<32>
    ) -> Option<OrderWithStorageData<AllOrders>> {
        self.pending_orders
            .get_mut(&pool_id)
            .and_then(|pool| {
                pool.remove_order(order_id)
                    .owned_map(|| self.metrics.decr_pending_orders(pool_id, 1))
            })
            .or_else(|| {
                self.parked_orders.get_mut(&pool_id).and_then(|pool| {
                    pool.remove_order(order_id)
                        .owned_map(|| self.metrics.decr_parked_orders(pool_id, 1))
                })
            })
    }

    pub fn get_all_orders(&self) -> Vec<OrderWithStorageData<AllOrders>> {
        self.pending_orders
            .values()
            .flat_map(|p| p.get_all_orders())
            .collect()
    }

    pub fn park_order(&mut self, order_id: &OrderId) {
        let Some(mut order) = self.remove_order(order_id.pool_id, order_id.hash) else { return };
        order.is_currently_valid = Some(UserAccountVerificationError::Unknown {
            err: "parked by other transaction".into()
        });
        self.add_order(order).unwrap();
    }

    pub fn new_pool(&mut self, pool: NewInitializedPool) {
        let old_is_none = self
            .pending_orders
            .insert(pool.id, PendingPool::new())
            .is_none()
            || self
                .parked_orders
                .insert(pool.id, ParkedPool::new())
                .is_none();

        assert!(old_is_none);
    }

    pub fn remove_invalid_order(&mut self, order_hash: B256) {
        self.pending_orders.iter_mut().for_each(|(pool_id, pool)| {
            if pool.remove_order(order_hash).is_some() {
                self.metrics.decr_pending_orders(*pool_id, 1);
            }
        });

        self.parked_orders.iter_mut().for_each(|(pool_id, pool)| {
            if pool.remove_order(order_hash).is_some() {
                self.metrics.decr_parked_orders(*pool_id, 1);
            }
        });
    }
}
