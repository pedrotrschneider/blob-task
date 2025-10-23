// src/continue_with.rs
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// A future that chains a new future after the first one completes.
    /// Created by the `ContinueWithExt::continue_with` method.
    #[must_use = "futures do nothing unless you .await or poll them"]
    pub struct ContinueWith<F1, F2, Fut>
    where
        F1: Future,
        F2: FnOnce(F1::Output) -> Fut,
        Fut: Future,
    {
        #[pin]
        first: F1,
        // The continuation closure, stored as an Option
        // so we can take it when the first future completes.
        continuation: Option<F2>,
        #[pin]
        // The second future, which is created by the continuation.
        second: Option<Fut>,
    }
}

impl<F1, F2, Fut> ContinueWith<F1, F2, Fut>
where
    F1: Future,
    F2: FnOnce(F1::Output) -> Fut,
    Fut: Future,
{
    /// Creates a new ContinueWith future
    fn new(first: F1, continuation: F2) -> Self {
        Self {
            first,
            continuation: Some(continuation),
            second: None,
        }
    }
}

impl<F1, F2, Fut> Future for ContinueWith<F1, F2, Fut>
where
    F1: Future,
    F2: FnOnce(F1::Output) -> Fut,
    Fut: Future,
{
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        // Check if we are already processing the second future
        if let Some(second) = this.second.as_mut().as_pin_mut() {
            // Poll the second future
            return second.poll(ctx);
        }

        // If not, poll the first future
        match this.first.poll(ctx) {
            Poll::Ready(result) => {
                // First future completed.
                // Take the continuation, call it to get the second future,
                // and pin it in place.
                let continuation = this
                    .continuation
                    .take()
                    .expect("ContinueWith polled after continuation was taken");

                let second_future = continuation(result); // Call FnOnce
                this.second.set(Some(second_future));

                // Poll the newly created second future
                this.second
                    .as_mut()
                    .as_pin_mut()
                    .expect("second future was just set")
                    .poll(ctx)
            }
            Poll::Pending => {
                // First future is not ready
                Poll::Pending
            }
        }
    }
}

/// Extension trait to add `continue_with` to all futures.
///
/// This allows chaining futures sequentially, where the second future
/// is created based on the result of the first.
pub trait ContinueWithExt: Future + Sized {
    /// Chains a new future to execute after the first one completes.
    ///
    /// The closure `f` is called with the result of the first future
    /// and returns a new future.
    ///
    /// # Example
    /// ```rust
    /// # use blob_task::ContinueWithExt;
    /// # use blob_task::CompletedTask;
    /// #
    /// # #[tokio::main]
    /// # async fn main() {
    ///     let result = CompletedTask::new(10)
    ///         .continue_with(|val| async move {
    ///             blob_task::wait_for_millis(50).await;
    ///             val * 2
    ///         })
    ///         .await;
    ///
    ///     assert_eq!(result, 20);
    /// # }
    /// ```
    fn continue_with<F2, Fut>(self, f: F2) -> ContinueWith<Self, F2, Fut>
    where
        F2: FnOnce(Self::Output) -> Fut,
        Fut: Future;
}

impl<F1> ContinueWithExt for F1
where
    F1: Future,
{
    fn continue_with<F2, Fut>(self, f: F2) -> ContinueWith<Self, F2, Fut>
    where
        F2: FnOnce(Self::Output) -> Fut,
        Fut: Future,
    {
        ContinueWith::new(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completed_task::CompletedTask;
    use crate::task_utils;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[tokio::test]
    async fn continue_with_simple_chain() {
        let result = CompletedTask::new(42)
            .continue_with(|val| async move { val + 1 })
            .await;

        assert_eq!(result, 43);
    }

    #[tokio::test]
    async fn continue_with_delay_in_first() {
        let result = (async {
            task_utils::wait_for_millis(50).await;
            return 10;
        })
            .continue_with(|val| async move { val * 2 })
            .await;

        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn continue_with_delay_in_second() {
        let start = std::time::Instant::now();

        let result = CompletedTask::new(10)
            .continue_with(|val| async move {
                task_utils::wait_for_millis(50).await;
                val * 2
            })
            .await;

        let elapsed = start.elapsed();
        assert_eq!(result, 20);
        assert!(elapsed >= Duration::from_millis(45));
    }

    #[tokio::test]
    async fn continue_with_both_delays() {
        let start = std::time::Instant::now();

        let result = (async {
            task_utils::wait_for_millis(50).await;
            return 10;
        })
            .continue_with(|val| async move {
                task_utils::wait_for_millis(50).await;
                val * 2
            })
            .await;

        let elapsed = start.elapsed();
        assert_eq!(result, 20);
        assert!(elapsed >= Duration::from_millis(95));
    }

    #[tokio::test]
    async fn continue_with_change_type() {
        let result = CompletedTask::new(42)
            .continue_with(|val| async move { format!("The value is {}", val) })
            .await;

        assert_eq!(result, "The value is 42");
    }

    #[tokio::test]
    async fn continue_with_from_string() {
        let result = CompletedTask::new(String::from("hello"))
            .continue_with(|val| async move {
                val.len()
            })
            .await;

        assert_eq!(result, 5);
    }

    #[tokio::test]
    async fn continue_with_unit_type() {
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = Arc::clone(&executed);

        CompletedTask::new(())
            .continue_with(move |_| async move {
                executed_clone.store(true, Ordering::SeqCst);
            })
            .await;

        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn continue_with_complex_chain() {
        let result = CompletedTask::new(2)
            .continue_with(|val| async move {
                task_utils::wait_for_millis(10).await;
                val * 2 // 4
            })
            .continue_with(|val| async move {
                task_utils::wait_for_millis(10).await;
                val + 1 // 5
            })
            .continue_with(|val| async move {
                task_utils::wait_for_millis(10).await;
                format!("Result: {}", val) // "Result: 5"
            })
            .await;

        assert_eq!(result, "Result: 5");
    }

    #[tokio::test]
    async fn continue_with_captures_environment() {
        let outside_value = Arc::new(AtomicUsize::new(10));
        let outside_value_clone = Arc::clone(&outside_value);

        let result = CompletedTask::new(5)
            .continue_with(move |val| async move {
                let outside = outside_value_clone.load(Ordering::SeqCst);
                val + outside // 5 + 10
            })
            .await;

        assert_eq!(result, 15);
    }

    #[tokio::test]
    #[should_panic(expected = "CompletedTask polled after completion")]
    async fn continue_with_panics_on_second_poll_after_completion() {
        use std::future::Future;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        // Helper to create a dummy waker for testing
        fn dummy_waker() -> Waker {
            fn no_op(_: *const ()) {}
            fn clone(_: *const ()) -> RawWaker { raw_waker() }
            fn raw_waker() -> RawWaker { RawWaker::new(std::ptr::null(), &VTABLE) }
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
            unsafe { Waker::from_raw(raw_waker()) }
        }

        let mut task = Box::pin(CompletedTask::new(42).continue_with(|v| CompletedTask::new(v)));
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        // First poll succeeds and completes
        assert_eq!(task.as_mut().poll(&mut ctx), Poll::Ready(42));

        // Second poll should panic
        let _ = task.as_mut().poll(&mut ctx);
    }

    #[tokio::test]
    async fn continue_with_on_pending_future() {
        let tcs = crate::TaskCompletionSource::new();
        let tcs_clone = tcs.clone();

        let task = tcs
            .task()
            .continue_with(|val| async move { val * 2 });

        let handle = tokio::spawn(task);

        // Give time for the task to poll and go to pending
        task_utils::wait_for_millis(50).await;

        // Complete the first future
        tcs_clone.complete(10);

        let result = handle.await.unwrap();
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn continue_with_needs_send() {
        // This test just needs to compile
        fn assert_send<T: Send>(_: T) {} // <-- Changed signature to take a value

        let task = CompletedTask::new(1).continue_with(|v| async move { v + 1 });
        assert_send(task); // <-- Pass the task value itself
    }

    #[tokio::test]
    async fn continue_with_using_fn_once() {
        let data = Arc::new(Mutex::new(Some(String::from("take me"))));

        let task = CompletedTask::new(1).continue_with(move |v| async move {
            // This closure can only be called once because it takes data
            let my_data = data.lock().unwrap().take().unwrap();
            format!("{} - {}", my_data, v)
        });

        let result = task.await;
        assert_eq!(result, "take me - 1");

        // Verify data was taken
        let data = Arc::new(Mutex::new(Some(String::from("take me"))));
        let task2 = CompletedTask::new(2).continue_with(move |v| async move {
            let my_data = data.lock().unwrap().take().unwrap();
            format!("{} - {}", my_data, v)
        });

        // Awaiting a second time would fail if it wasn't FnOnce
        // (but we can't easily test the poll, so we just check it runs)
        let handle = tokio::spawn(task2);
        let result2 = handle.await.unwrap();
        assert_eq!(result2, "take me - 2");
    }
}


