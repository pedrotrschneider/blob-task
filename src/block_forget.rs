use tokio::runtime::{Handle, Runtime};

/// Synchronously wait for a future to complete.
///
/// **Important:** Must **not** be called from inside an async function.
pub trait Block<T>
where
    T: Clone + Send + 'static,
{
    fn block(self) -> T;
}

impl<F, T> Block<T> for F
where
    F: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    fn block(self) -> T {
        if let Ok(handle) = Handle::try_current() {
            handle.block_on(self)
        } else {
            let rt = Runtime::new().expect("Failed to create Tokio runtime");
            rt.block_on(self)
        }
    }
}

pub trait Forget {
    fn forget(self);
}

impl<F, T> Forget for F
where
    F: Future<Output = T> + Send + 'static,
    T: Clone + Send + 'static,
{
    fn forget(self) {
        // If runtime exists, spawn on it
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(self);
        } else {
            // No runtime exists → create a temporary runtime on a new thread
            std::thread::spawn(move || {
                let rt = Runtime::new().expect("Failed to create Tokio runtime");
                rt.block_on(self);
            });
        }
    }
}

#[cfg(test)]
mod block_forget_tests {
    use crate::block_forget::{Block, Forget};
    use crate::task_utils;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn block_simple_future() {
        let result = (async { 42 }).block();
        assert_eq!(result, 42);
    }

    #[test]
    fn block_with_delay() {
        let start = std::time::Instant::now();
        let result = (async {
            task_utils::wait_for_millis(100).await;
            return "done";
        })
            .block();
        let elapsed = start.elapsed();

        assert_eq!(result, "done");
        assert!(elapsed >= Duration::from_millis(95));
    }

    #[test]
    fn block_with_string() {
        let result = (async { String::from("hello world") }).block();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn block_with_complex_computation() {
        let result = (async {
            let mut sum = 0;
            for i in 0..100 {
                sum += i;
                if i % 20 == 0 {
                    task_utils::yield_once().await;
                }
            }
            return sum;
        })
            .block();

        assert_eq!(result, 4950);
    }

    #[test]
    fn block_with_complex_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct Data {
            id: i32,
            message: String,
        }

        let result = (async {
            Data {
                id: 1,
                message: String::from("test"),
            }
        })
            .block();

        assert_eq!(result.id, 1);
        assert_eq!(result.message, "test");
    }

    #[test]
    fn forget_executes_future() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = Arc::clone(&executed);

        (async move {
            task_utils::wait_for_millis(50).await;
            executed_clone.store(true, Ordering::SeqCst);
        })
            .forget();

        // Give it time to execute
        std::thread::sleep(Duration::from_millis(150));

        assert!(executed.load(Ordering::SeqCst));
    }

    #[test]
    fn forget_doesnt_block() {
        let start = std::time::Instant::now();

        (async {
            task_utils::wait_for_millis(200).await;
        })
            .forget();

        let elapsed = start.elapsed();

        // Should return immediately, not wait for 200ms
        assert!(elapsed < Duration::from_millis(50));
    }

    #[test]
    fn forget_multiple_futures() {
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let counter_clone = Arc::clone(&counter);
            (async move {
                task_utils::wait_for_millis(50).await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
                .forget();
        }

        // Give them time to execute
        std::thread::sleep(Duration::from_millis(150));

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn block_with_result_type() {
        let result: Result<i32, &str> = (async { Ok(42) }).block();
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn block_with_option_type() {
        let result: Option<i32> = (async { Some(100) }).block();
        assert_eq!(result, Some(100));
    }

    #[test]
    fn block_multiple_sequential_calls() {
        let r1 = (async { 1 }).block();
        let r2 = (async { 2 }).block();
        let r3 = (async { 3 }).block();

        assert_eq!(r1 + r2 + r3, 6);
    }

    #[test]
    fn forget_with_side_effects() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        (async move {
            for i in 0..5 {
                counter_clone.fetch_add(i, Ordering::SeqCst);
                task_utils::wait_for_millis(10).await;
            }
        })
            .forget();

        // Give it time to execute
        std::thread::sleep(Duration::from_millis(100));

        assert_eq!(counter.load(Ordering::SeqCst), 0 + 1 + 2 + 3 + 4);
    }

    #[test]
    fn block_with_cancellation_token() {
        use crate::CancellationToken;

        let token = CancellationToken::new();
        let token_clone = token.clone();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            token_clone.cancel();
        });

        let result = (async move {
            tokio::select! {
                _ = token.cancelled_future() => {
                    return "cancelled";
                }
                _ = task_utils::wait_for_millis(200) => {
                    return "timeout";
                }
            }
        })
            .block();

        assert_eq!(result, "cancelled");
    }

    #[test]
    fn block_with_task_completion_source() {
        use crate::TaskCompletionSource;

        let tcs = TaskCompletionSource::new();
        let tcs_clone = tcs.clone();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            tcs_clone.complete(999);
        });

        let result = tcs.task().block();
        assert_eq!(result, 999);
    }

    #[test]
    fn forget_doesnt_wait_for_completion() {
        let start = std::time::Instant::now();

        for _ in 0..100 {
            (async {
                task_utils::wait_for_millis(1000).await;
            })
                .forget();
        }

        let elapsed = start.elapsed();

        // Should finish very quickly despite launching 100 long-running futures
        assert!(elapsed < Duration::from_millis(500));
    }

    #[test]
    fn block_with_blob_task() {
        use crate::BlobTask;

        let blob = BlobTask::from_future(async { 555 });
        let result = blob.block();
        assert_eq!(result, 555);
    }

    #[test]
    fn forget_with_blob_task() {
        use crate::BlobTask;

        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = Arc::clone(&executed);

        let blob = BlobTask::from_future(async move {
            task_utils::wait_for_millis(50).await;
            executed_clone.store(true, Ordering::SeqCst);
        });

        blob.forget();

        std::thread::sleep(Duration::from_millis(150));
        assert!(executed.load(Ordering::SeqCst));
    }
}
