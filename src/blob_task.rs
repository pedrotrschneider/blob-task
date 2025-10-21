use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use tokio::runtime::{Handle, Runtime};

/// Shared internal state of the BlobTask.
struct BlobTaskSharedState<T>
where
    T: Clone + Send + 'static,
{
    result: Option<T>,
    wakers: Vec<Waker>,
    future: Option<Pin<Box<dyn Future<Output = T> + Send>>>, // not spawned yet
}

/// A clonable, awaitable wrapper that runs its inner future exactly once.
///
/// Each clone can `.await` it independently; they all receive the same result.
pub struct BlobTask<T>
where
    T: Clone + Send + 'static,
{
    state: Arc<Mutex<BlobTaskSharedState<T>>>,
}

impl<T> BlobTask<T>
where
    T: Clone + Send + 'static,
{
    /// Wrap any future in a shared, clonable task.
    pub fn from_future<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        let state = BlobTaskSharedState {
            result: None,
            wakers: Vec::new(),
            future: Some(Box::pin(future)),
        };

        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Spawn the future in a runtime if it hasn't been spawned yet
    fn spawn_if_needed(&self) {
        let maybe_fut = {
            let mut state = self.state.lock().unwrap();
            state.future.take()
        };

        if let Some(fut) = maybe_fut {
            let state_clone = self.state.clone();
            if let Ok(handle) = Handle::try_current() {
                handle.spawn(async move {
                    let result = fut.await;
                    let mut state = state_clone.lock().unwrap();
                    state.result = Some(result);
                    for w in state.wakers.drain(..) {
                        w.wake();
                    }
                });
            } else {
                // no runtime → spawn a thread with a full-featured runtime
                std::thread::spawn(move || {
                    let rt = Runtime::new().expect("Failed to create Tokio runtime");
                    rt.block_on(async move {
                        let result = fut.await;
                        let mut state = state_clone.lock().unwrap();
                        state.result = Some(result);
                        for w in state.wakers.drain(..) {
                            w.wake();
                        }
                    });
                });
            }
        }
    }
}

impl<T> Clone for BlobTask<T>
where
    T: Clone + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

// Implement Future for BlobTask
impl<T> Future for BlobTask<T>
where
    T: Clone + Send + 'static,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<T> {
        self.spawn_if_needed();

        let mut state = self.state.lock().unwrap();
        if let Some(result) = &state.result {
            Poll::Ready(result.clone())
        } else {
            state.wakers.push(ctx.waker().clone());
            Poll::Pending
        }
    }
}

pub trait ToBlobTaskExt: Future + Sized {
    fn to_blob_task(self) -> BlobTask<Self::Output>
    where
        Self: Send + 'static,
        <Self as Future>::Output: Clone + Send + 'static,
    {
        BlobTask::from_future(self)
    }
}

// Implement it for all futures
impl<F: Future> ToBlobTaskExt for F {}

#[cfg(test)]
mod blob_task_tests {
    use crate::blob_task::{BlobTask, ToBlobTaskExt};
    use crate::task_utils;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn blob_task_basic_execution() {
        let task = BlobTask::from_future(async { 42 });
        let result = task.await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn blob_task_with_delay() {
        let start = std::time::Instant::now();
        let task = BlobTask::from_future(async {
            task_utils::wait_for_millis(100).await;
            return "completed";
        });
        let result = task.await;
        let elapsed = start.elapsed();

        assert_eq!(result, "completed");
        assert!(elapsed >= Duration::from_millis(95));
    }

    #[tokio::test]
    async fn blob_task_cloning() {
        let task = BlobTask::from_future(async { 100 });
        let task_clone1 = task.clone();
        let task_clone2 = task.clone();

        let (r1, r2, r3) = tokio::join!(task, task_clone1, task_clone2);

        assert_eq!(r1, 100);
        assert_eq!(r2, 100);
        assert_eq!(r3, 100);
    }

    #[tokio::test]
    async fn blob_task_runs_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let task = BlobTask::from_future(async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            return 42;
        });

        let task_clone1 = task.clone();
        let task_clone2 = task.clone();

        let (r1, r2, r3) = tokio::join!(task, task_clone1, task_clone2);

        assert_eq!(r1, 42);
        assert_eq!(r2, 42);
        assert_eq!(r3, 42);
        // Should only run once despite multiple awaits
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn blob_task_with_string() {
        let task = BlobTask::from_future(async { String::from("hello") });
        let result = task.await;
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn blob_task_extension_trait() {
        let future = async { 999 };
        let task = future.to_blob_task();
        let result = task.await;
        assert_eq!(result, 999);
    }

    #[tokio::test]
    async fn blob_task_with_complex_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct Data {
            id: i32,
            name: String,
        }

        let task = BlobTask::from_future(async {
            Data {
                id: 1,
                name: String::from("test"),
            }
        });

        let result = task.await;
        assert_eq!(result.id, 1);
        assert_eq!(result.name, "test");
    }

    #[tokio::test]
    async fn blob_task_multiple_sequential_awaits() {
        let task = BlobTask::from_future(async { 50 });

        let r1 = task.clone().await;
        let r2 = task.clone().await;
        let r3 = task.await;

        assert_eq!(r1, 50);
        assert_eq!(r2, 50);
        assert_eq!(r3, 50);
    }

    #[tokio::test]
    async fn blob_task_with_unit_type() {
        let task = BlobTask::from_future(async { () });
        task.await;
        assert!(true);
    }

    #[tokio::test]
    async fn blob_task_concurrent_spawns() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let task = BlobTask::from_future(async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            task_utils::wait_for_millis(50).await;
            return 42;
        });

        let mut handles = vec![];
        for _ in 0..10 {
            let task_clone = task.clone();
            let handle = tokio::spawn(async move { task_clone.await });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert_eq!(result, 42);
        }

        // Should only execute once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn blob_task_extension_trait_with_async_block() {
        let result = (async {
            task_utils::wait_for_millis(10).await;
            return 123;
        })
        .to_blob_task()
        .await;

        assert_eq!(result, 123);
    }

    #[tokio::test]
    async fn blob_task_nested_awaits() {
        let outer_task = BlobTask::from_future(async {
            let inner_task = BlobTask::from_future(async { 10 });
            let value = inner_task.await;
            return value * 2;
        });

        let result = outer_task.await;
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn blob_task_with_result_type() {
        let task = BlobTask::from_future(async {
            task_utils::wait_for_millis(10).await;
            return Result::<i32, &str>::Ok(42);
        });

        let result = task.await;
        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn blob_task_share_across_tasks() {
        let task = BlobTask::from_future(async { String::from("shared") });

        let handle1 = tokio::spawn({
            let task = task.clone();
            async move { task.await }
        });

        let handle2 = tokio::spawn({
            let task = task.clone();
            async move { task.await }
        });

        let (r1, r2) = tokio::join!(handle1, handle2);

        assert_eq!(r1.unwrap(), "shared");
        assert_eq!(r2.unwrap(), "shared");
    }

    #[test]
    fn blob_task_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<BlobTask<i32>>();
    }

    #[test]
    fn blob_task_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<BlobTask<i32>>();
    }

    #[tokio::test]
    async fn blob_task_from_completed_task() {
        let completed = crate::CompletedTask::new(777);
        let blob = completed.to_blob_task();
        let result = blob.await;
        assert_eq!(result, 777);
    }
}
