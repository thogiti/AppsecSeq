use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap}
};

use alloy::primitives::FixedBytes;
use angstrom_types::{
    orders::OrderPriorityData, sol_bindings::grouped_orders::OrderWithStorageData
};

pub struct PendingPool<Order: Clone> {
    /// all order hashes
    orders: HashMap<FixedBytes<32>, OrderWithStorageData<Order>>,
    /// bids are sorted descending by price, TODO: This should be binned into
    /// ticks based off of the underlying pools params
    bids:   BTreeMap<Reverse<OrderPriorityData>, FixedBytes<32>>,
    /// asks are sorted ascending by price,  TODO: This should be binned into
    /// ticks based off of the underlying pools params
    asks:   BTreeMap<OrderPriorityData, FixedBytes<32>>
}

impl<Order: Clone> PendingPool<Order> {
    #[allow(unused)]
    pub fn new() -> Self {
        Self { orders: HashMap::new(), bids: BTreeMap::new(), asks: BTreeMap::new() }
    }

    pub fn get_order(&self, id: FixedBytes<32>) -> Option<OrderWithStorageData<Order>> {
        self.orders.get(&id).cloned()
    }

    pub fn add_order(&mut self, order: OrderWithStorageData<Order>) {
        if order.is_bid {
            self.bids
                .insert(Reverse(order.priority_data), order.order_id.hash);
        } else {
            self.asks.insert(order.priority_data, order.order_id.hash);
        }
        self.orders.insert(order.order_id.hash, order);
    }

    pub fn remove_order(&mut self, id: FixedBytes<32>) -> Option<OrderWithStorageData<Order>> {
        let order = self.orders.remove(&id)?;

        if order.is_bid {
            self.bids.remove(&Reverse(order.priority_data))?;
        } else {
            self.asks.remove(&order.priority_data)?;
        }

        // probably fine to strip extra data here
        Some(order)
    }

    pub fn get_all_orders(&self) -> Vec<OrderWithStorageData<Order>> {
        self.orders.values().cloned().collect()
    }
}
