use tokio::runtime::{Handle, Runtime};
use std::future::Future;
use std::sync::{Arc, Mutex, Condvar}; // Import sync primitives

/// Synchronously wait for a future to complete.
///
/// If called from within an existing Tokio runtime, it will
/// spawn the task and use `block_in_place` to wait for its
/// completion without stalling the executor.
///
/// If called from a non-async context, it will create a new
/// temporary runtime and block on it.
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
            // We are inside an existing runtime.
            // We must not call handle.block_on() or create a new runtime.
            //
            // 1. Create an Arc/Mutex/Condvar to receive the result.
            // 2. Spawn the future onto the existing runtime.
            // 3. Use `block_in_place` to wait on the Condvar.

            let pair = Arc::new((Mutex::new(None::<T>), Condvar::new()));
            let (lock, cvar) = &*pair;

            let pair_clone = Arc::clone(&pair);
            handle.spawn(async move {
                let result = self.await;
                let (lock, cvar) = &*pair_clone;
                let mut guard = lock.lock().unwrap();
                *guard = Some(result);
                // Notify the waiting thread that the result is ready
                cvar.notify_one();
            });

            // Tell Tokio that this thread is about to block
            tokio::task::block_in_place(move || {
                let mut guard = lock.lock().unwrap();
                // Wait until the spawned task notifies us
                while guard.is_none() {
                    guard = cvar.wait(guard).unwrap();
                }
                // The guard now contains Some(result). Take it.
                guard.take().unwrap()
            })

        } else {
            // We are not inside a runtime. This is the simple case.
            // Create a new runtime and block on it.
            let rt = Runtime::new().expect("Failed to create Tokio runtime");
            rt.block_on(self)
        }
    }
}

/// Spawns a future to run in the background.
///
/// If called from within an existing Tokio runtime, it spawns
/// onto that runtime.
///
/// If called from a non-async context, it will create a new
/// temporary runtime on a new thread and run the future there.
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    use crate::ToBlobTaskExt; // Import for test

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

    #[tokio::test(flavor = "multi_thread")] // Use the multithreaded runtime
    async fn block_from_within_a_runtime_context() {
        async fn test_async() -> i32 {
            task_utils::wait_for_millis(50).await;
            3
        }

        fn test_sync() -> i32 {
            // This .block() call is happening *inside* an active runtime
            let result = test_async().to_blob_task().block();
            result
        }

        // We are inside #[tokio::test]
        let result = test_sync();
        assert_eq!(result, 3);
    }

    // Another test for forget from within a runtime
    #[tokio::test(flavor = "multi_thread")] // Use the multithreaded runtime
    async fn forget_from_within_a_runtime_context() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        fn test_sync_forget(counter: Arc<AtomicUsize>) {
            // This .forget() call is happening *inside* an active runtime
            (async move {
                task_utils::wait_for_millis(50).await;
                counter.fetch_add(1, Ordering::SeqCst);
            }).forget();
        }

        test_sync_forget(counter_clone);

        // The .forget() should have spawned onto the current runtime
        // so we can just wait for it.
        task_utils::wait_for_millis(100).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}

