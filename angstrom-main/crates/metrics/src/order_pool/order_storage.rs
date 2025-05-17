use prometheus::IntGauge;

use crate::METRICS_ENABLED;

#[derive(Clone)]
struct OrderStorageMetrics {
    // number of vanilla limit orders
    vanilla_limit_orders:        IntGauge,
    // number of composable limit orders
    composable_limit_orders:     IntGauge,
    // number of searcher orders
    searcher_orders:             IntGauge,
    // number of pending finalization orders
    pending_finalization_orders: IntGauge,
    // number of cancelled vanilla orders
    cancelled_vanilla_orders:    IntGauge,
    // number of cancelled composable orders
    cancelled_composable_orders: IntGauge,
    // number of cancelled searcher orders
    cancelled_searcher_orders:   IntGauge
}

impl Default for OrderStorageMetrics {
    fn default() -> Self {
        let vanilla_limit_orders = prometheus::register_int_gauge!(
            "order_storage_vanilla_limit_orders",
            "number of vanilla limit orders",
        )
        .unwrap();

        let composable_limit_orders = prometheus::register_int_gauge!(
            "order_storage_composable_limit_orders",
            "number of composable limit orders",
        )
        .unwrap();

        let searcher_orders = prometheus::register_int_gauge!(
            "order_storage_searcher_orders",
            "number of searcher orders",
        )
        .unwrap();

        let pending_finalization_orders = prometheus::register_int_gauge!(
            "order_storage_pending_finalization_orders",
            "number of pending finalization orders",
        )
        .unwrap();

        let cancelled_vanilla_orders = prometheus::register_int_gauge!(
            "order_storage_cancelled_vanilla_orders",
            "number of cancelled vanilla orders",
        )
        .unwrap();

        let cancelled_composable_orders = prometheus::register_int_gauge!(
            "order_storage_cancelled_composable_orders",
            "number of cancelled composable orders",
        )
        .unwrap();

        let cancelled_searcher_orders = prometheus::register_int_gauge!(
            "order_storage_cancelled_searcher_orders",
            "number of cancelled searcher orders",
        )
        .unwrap();

        Self {
            vanilla_limit_orders,
            searcher_orders,
            pending_finalization_orders,
            composable_limit_orders,
            cancelled_vanilla_orders,
            cancelled_composable_orders,
            cancelled_searcher_orders
        }
    }
}

impl OrderStorageMetrics {
    pub fn incr_vanilla_limit_orders(&self, count: usize) {
        self.vanilla_limit_orders.add(count as i64);
    }

    pub fn decr_vanilla_limit_orders(&self, count: usize) {
        self.vanilla_limit_orders.sub(count as i64);
    }

    pub fn incr_composable_limit_orders(&self, count: usize) {
        self.composable_limit_orders.add(count as i64);
    }

    pub fn decr_composable_limit_orders(&self, count: usize) {
        self.composable_limit_orders.sub(count as i64);
    }

    pub fn incr_searcher_orders(&self, count: usize) {
        self.searcher_orders.add(count as i64);
    }

    pub fn decr_searcher_orders(&self, count: usize) {
        self.searcher_orders.sub(count as i64);
    }

    pub fn incr_pending_finalization_orders(&self, count: usize) {
        self.pending_finalization_orders.add(count as i64);
    }

    pub fn decr_pending_finalization_orders(&self, count: usize) {
        self.pending_finalization_orders.sub(count as i64);
    }

    pub fn incr_cancelled_vanilla_orders(&self, count: usize) {
        self.cancelled_vanilla_orders.add(count as i64);
    }

    pub fn incr_cancelled_composable_orders(&self, count: usize) {
        self.cancelled_composable_orders.add(count as i64);
    }

    pub fn incr_cancelled_searcher_orders(&self, count: usize) {
        self.cancelled_searcher_orders.add(count as i64);
    }
}

#[derive(Clone)]
pub struct OrderStorageMetricsWrapper(Option<OrderStorageMetrics>);

impl Default for OrderStorageMetricsWrapper {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderStorageMetricsWrapper {
    pub fn new() -> Self {
        Self(
            METRICS_ENABLED
                .get()
                .copied()
                .unwrap_or_default()
                .then(OrderStorageMetrics::default)
        )
    }

    pub fn incr_vanilla_limit_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.incr_vanilla_limit_orders(count)
        }
    }

    pub fn decr_vanilla_limit_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.decr_vanilla_limit_orders(count)
        }
    }

    pub fn incr_composable_limit_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.incr_composable_limit_orders(count)
        }
    }

    pub fn incr_cancelled_vanilla_orders(&self) {
        if let Some(this) = self.0.as_ref() {
            this.incr_cancelled_vanilla_orders(1)
        }
    }

    pub fn incr_cancelled_composable_orders(&self) {
        if let Some(this) = self.0.as_ref() {
            this.incr_cancelled_composable_orders(1)
        }
    }

    pub fn incr_cancelled_searcher_orders(&self) {
        if let Some(this) = self.0.as_ref() {
            this.incr_cancelled_searcher_orders(1)
        }
    }

    pub fn decr_composable_limit_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.decr_composable_limit_orders(count)
        }
    }

    pub fn incr_searcher_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.incr_searcher_orders(count)
        }
    }

    pub fn decr_searcher_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.decr_searcher_orders(count)
        }
    }

    pub fn incr_pending_finalization_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.incr_pending_finalization_orders(count)
        }
    }

    pub fn decr_pending_finalization_orders(&self, count: usize) {
        if let Some(this) = self.0.as_ref() {
            this.decr_pending_finalization_orders(count)
        }
    }
}
