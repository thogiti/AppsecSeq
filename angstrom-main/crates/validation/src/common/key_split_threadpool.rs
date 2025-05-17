use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::Arc,
    task::{Poll, Waker}
};

use angstrom_metrics::validation::ValidationMetrics;
use angstrom_utils::{PollExt, sync_pipeline::ThreadPool};
use futures::{Stream, StreamExt, stream::FuturesUnordered};
use tokio::sync::Semaphore;

type PendingFut<F> = Pin<Box<dyn Future<Output = <F as Future>::Output> + Send + Sync>>;

pub struct KeySplitThreadpool<K: PartialEq + Eq + Hash + Clone, F: Future, TP: ThreadPool> {
    tp:              TP,
    pending_results: FuturesUnordered<PendingFut<F>>,
    permit_size:     usize,
    pending:         HashMap<K, Arc<Semaphore>>,
    waker:           Option<Waker>,
    metrics:         ValidationMetrics
}

impl<K: PartialEq + Eq + Hash + Clone, F: Future, TP: ThreadPool> KeySplitThreadpool<K, F, TP>
where
    K: Send + Unpin + 'static,
    F: Send + Sync + 'static + Unpin,
    TP: Clone + Send + Sync + 'static + Unpin,
    <F as Future>::Output: Send + Sync + 'static + Unpin
{
    pub fn new(theadpool: TP, permit_size: usize) -> Self {
        Self {
            tp: theadpool,
            permit_size,
            pending: HashMap::default(),
            pending_results: FuturesUnordered::default(),
            metrics: ValidationMetrics::new(),
            waker: None
        }
    }

    pub fn spawn_raw(&mut self, fut: F) {
        let tp_cloned = self.tp.clone();
        let fut = Box::pin(async move { tp_cloned.spawn(fut).await }) as PendingFut<F>;

        self.pending_results.push(fut);
        // if a waker is scheduled. insure we pool
        self.waker.as_ref().inspect(|i| i.wake_by_ref());
    }

    pub fn add_new_task(&mut self, key: K, fut: F) {
        // grab semaphore
        let permit = self
            .pending
            .entry(key)
            .or_insert_with(|| Arc::new(Semaphore::new(self.permit_size)));
        let permit_cloned = permit.clone();
        let tp_cloned = self.tp.clone();
        let metrics = self.metrics.clone();

        let fut = Box::pin(async move {
            let permit = metrics
                .measure_wait_time(|| {
                    Box::pin(async { permit_cloned.acquire().await.expect("never") })
                })
                .await;

            let res = tp_cloned.spawn(fut).await;
            drop(permit);

            res
        }) as PendingFut<F>;

        self.pending_results.push(fut);
        // if a waker is scheduled. insure we pool
        self.waker.as_ref().inspect(|i| i.wake_by_ref());
    }

    /// registers waker if its doesn't exist
    pub fn try_register_waker(&mut self, f: impl FnOnce() -> Waker) {
        if self.waker.is_none() {
            self.waker = Some(f());
        }
    }
}

impl<K: PartialEq + Eq + Hash + Clone, F: Future, TP: ThreadPool> Stream
    for KeySplitThreadpool<K, F, TP>
where
    K: Send + Unpin + 'static,
    F: Send + Sync + 'static + Unpin,
    TP: Clone,
    <F as Future>::Output: Send + Sync + 'static + Unpin
{
    type Item = F::Output;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> Poll<Option<Self::Item>> {
        self.pending_results
            .poll_next_unpin(cx)
            .filter(|inner| inner.is_some())
    }
}
