use crate::{BlobTask, ToBlobTaskExt};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

/// Internal state for BlobLazyTask.
/// Starts as `Factory` and transitions to `Task` on first poll.
enum LazyState<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    /// The factory function has not been run yet.
    Factory(Option<F>),
    /// The factory has been run, and this is the task to await.
    Task(BlobTask<T>),
}

/// A lazy, awaitable task that runs its async factory function only once.
///
/// The first time this future is awaited, it will run the provided factory
/// function to create a `BlobTask`. All subsequent awaits (even on clones)
/// will await the *same* underlying `BlobTask`, ensuring the factory
/// runs exactly once.
///
/// # Example
/// ```rust,no_run
/// # use blob_task::{BlobLazyTask, wait_for_millis};
/// # use std::sync::atomic::{AtomicUsize, Ordering};
/// # use std::sync::Arc;
/// #
/// # #[tokio::main]
/// # async fn main() {
///     // This can be stored in a static, e.g., using `once_cell::sync::Lazy`.
///     // For this example, we'll create it locally.
///     let counter = Arc::new(AtomicUsize::new(0));
///
///     // We must clone `counter` for the factory closure
///     let counter_clone = counter.clone();
///     let lazy_data = BlobLazyTask::new(|| async move {
///         println!("Initializing data...");
///         counter_clone.fetch_add(1, Ordering::SeqCst);
///         wait_for_millis(50).await;
///         "lazy data".to_string()
///     });
///
///     // The factory has not run yet.
///     assert_eq!(counter.load(Ordering::SeqCst), 0);
///
///     // Run two tasks concurrently
///     let (r1, r2) = tokio::join!(
///         lazy_data.clone(),
///         lazy_data.clone()
///     );
///
///     // The factory ran exactly once.
///     assert_eq!(counter.load(Ordering::SeqCst), 1);
///     assert_eq!(r1, "lazy data");
///     assert_eq!(r2, "lazy data");
/// # }
/// ```
pub struct BlobLazyTask<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    state: Arc<Mutex<LazyState<T, F, Fut>>>,
}

impl<T, F, Fut> BlobLazyTask<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    /// Creates a new lazy task from an async factory function.
    ///
    /// The factory will not be run until the first time
    /// this task is `.await`ed.
    ///
    /// This function can be used to initialize `static` variables
    /// (e.g., with `once_cell`).
    pub fn new(factory: F) -> Self {
        Self {
            state: Arc::new(Mutex::new(LazyState::Factory(Some(factory)))),
        }
    }
}

impl<T, F, Fut> Future for BlobLazyTask<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut guard = self.state.lock().unwrap();

        // Get or create the task.
        // We must do this in a single match block to convince the borrow
        // checker that the mutable reference lives long enough.
        let task = match &mut *guard {
            LazyState::Task(task) => {
                // Task already exists, just poll it.
                return Pin::new(task).poll(ctx);
            }
            LazyState::Factory(factory_opt) => {
                // Factory exists, run it to create the task.
                let factory = factory_opt
                    .take()
                    .expect("BlobLazyTask factory was already taken. This is a bug.");
                let future = factory();
                let task = future.to_blob_task();

                // Replace the factory with the new task.
                *guard = LazyState::Task(task);

                // Now, get a mutable ref to the task we just inserted.
                if let LazyState::Task(task) = &mut *guard {
                    task
                } else {
                    // This is logically impossible as we just set it.
                    unreachable!()
                }
            }
        };

        // Poll the newly created task.
        Pin::new(task).poll(ctx)
    }
}

impl<T, F, Fut> Clone for BlobLazyTask<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

// The future is unpin because it just holds an Arc<Mutex<...>>.
// The actual state is heap-allocated and its location is stable.
impl<T, F, Fut> Unpin for BlobLazyTask<T, F, Fut>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_utils;
    use std::future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn lazy_task_runs_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // FIX: Wrap async block in a closure
        let lazy_task = BlobLazyTask::new(|| async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            task_utils::wait_for_millis(50).await;
            "done".to_string()
        });

        assert_eq!(counter.load(Ordering::SeqCst), 0);

        let (r1, r2) = tokio::join!(lazy_task.clone(), lazy_task.clone());

        assert_eq!(r1, "done");
        assert_eq!(r2, "done");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn lazy_task_is_lazy() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // FIX: Wrap async block in a closure
        let lazy_task = BlobLazyTask::new(|| async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            42
        });

        // Factory should not have run yet
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        task_utils::wait_for_millis(20).await;

        // Still should not have run
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Now we await it
        let result = lazy_task.await;

        assert_eq!(result, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn lazy_task_sequential_awaits() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // FIX: Wrap async block in a closure
        let lazy_task = BlobLazyTask::new(|| async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            100
        });

        let r1 = lazy_task.clone().await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let r2 = lazy_task.clone().await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        assert_eq!(r1, 100);
        assert_eq!(r2, 100);
    }

    #[tokio::test]
    async fn lazy_task_with_unit_type() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // FIX: Wrap async block in a closure
        let lazy_task = BlobLazyTask::new(|| async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        lazy_task.clone().await;
        lazy_task.clone().await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn lazy_task_factory_is_sync() {
        // Ensure it works with a non-async factory that returns a future
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let lazy_task = BlobLazyTask::new(move || {
            // This part is sync
            counter_clone.fetch_add(1, Ordering::SeqCst);
            // This part is async
            async { 42 }
        });

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        let result = lazy_task.await;
        assert_eq!(result, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lazy_task_is_send_sync() {
        fn assert_send<T: Send>(_: T) {}
        fn assert_sync<T: Sync>(_: T) {}

        // FIX: Remove helper and create inline, wrapping in a closure
        let lazy_task = BlobLazyTask::new(|| async { 42 });

        assert_send(lazy_task.clone()); // Test Send
        assert_sync(lazy_task); // Test Sync
    }

    #[test]
    fn lazy_task_is_unpin() {
        fn assert_unpin<T: Unpin>(_: T) {}

        // FIX: Remove helper and create inline, wrapping in a closure
        let lazy_task = BlobLazyTask::new(|| async { 42 });

        assert_unpin(lazy_task);
    }

    #[tokio::test]
    async fn lazy_task_factory_returns_completed_task() {
        // Ensure it works with a factory that returns an
        // immediately-ready future
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let lazy_task = BlobLazyTask::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            // This is not an `async` block, it just returns a future
            future::ready(99)
        });

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        let result = lazy_task.await;
        assert_eq!(result, 99);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}

