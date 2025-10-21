// timeout.rs
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{Sleep, sleep};

/// Error type returned when a timeout occurs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// A future that completes with an error if the inner future doesn't complete within the timeout.
pub struct Timeout<F>
where
    F: Future,
{
    future: Pin<Box<F>>,
    delay: Pin<Box<Sleep>>,
}

impl<F> Timeout<F>
where
    F: Future,
{
    /// Creates a new timeout future
    pub fn new(future: F, duration: Duration) -> Self {
        return Self {
            future: Box::pin(future),
            delay: Box::pin(sleep(duration)),
        };
    }

    /// Creates a timeout from milliseconds
    #[inline]
    pub fn millis(future: F, millis: u64) -> Self {
        return Self::new(future, Duration::from_millis(millis));
    }

    /// Creates a timeout from seconds
    #[inline]
    pub fn seconds(future: F, secs: u64) -> Self {
        return Self::new(future, Duration::from_secs(secs));
    }
}

impl<F> Future for Timeout<F>
where
    F: Future,
{
    type Output = Result<F::Output, TimeoutError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Try to poll the inner future first
        match self.future.as_mut().poll(cx) {
            Poll::Ready(value) => return Poll::Ready(Ok(value)),
            Poll::Pending => {}
        }

        // Check if timeout has elapsed
        return match self.delay.as_mut().poll(cx) {
            Poll::Ready(_) => Poll::Ready(Err(TimeoutError)),
            Poll::Pending => Poll::Pending,
        };
    }
}

/// Extension trait to add timeout methods to futures
pub trait TimeoutExt: Future + Sized {
    /// Adds a timeout to this future
    fn timeout(self, duration: Duration) -> Timeout<Self> {
        return Timeout::new(self, duration);
    }

    /// Adds a timeout in milliseconds
    fn timeout_millis(self, millis: u64) -> Timeout<Self> {
        return Timeout::millis(self, millis);
    }

    /// Adds a timeout in seconds
    fn timeout_seconds(self, secs: u64) -> Timeout<Self> {
        return Timeout::seconds(self, secs);
    }
}

// Implement for all futures
impl<F: Future> TimeoutExt for F {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WhenAny;
    use crate::blob_task::ToBlobTaskExt;
    use crate::task_utils;
    use std::time::Instant;
    use tokio::time::timeout;

    #[tokio::test]
    async fn timeout_completes_before_deadline() {
        let result = async {
            task_utils::wait_for_millis(50).await;
            return 42;
        }
        .timeout(Duration::from_millis(100))
        .await;

        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn timeout_exceeds_deadline() {
        let result = async {
            task_utils::wait_for_millis(200).await;
            return 42;
        }
        .timeout(Duration::from_millis(50))
        .await;

        assert_eq!(result, Err(TimeoutError));
    }

    #[tokio::test]
    async fn timeout_millis_helper() {
        let result = async {
            task_utils::wait_for_millis(50).await;
            return "success";
        }
        .timeout_millis(100)
        .await;

        assert_eq!(result, Ok("success"));
    }

    #[tokio::test]
    async fn timeout_seconds_helper() {
        let result = async {
            task_utils::wait_for_millis(50).await;
            return true;
        }
        .timeout_seconds(1)
        .await;

        assert_eq!(result, Ok(true));
    }

    #[tokio::test]
    async fn timeout_extension_trait() {
        let result = (async {
            task_utils::wait_for_millis(50).await;
            return 99;
        })
        .timeout(Duration::from_millis(200))
        .await;

        assert_eq!(result, Ok(99));
    }

    #[tokio::test]
    async fn timeout_extension_trait_exceeds() {
        let result = (async {
            task_utils::wait_for_millis(200).await;
            return 99;
        })
        .timeout(Duration::from_millis(50))
        .await;

        assert_eq!(result, Err(TimeoutError));
    }

    #[tokio::test]
    async fn timeout_millis_extension() {
        let result = (async { 123 }).timeout_millis(100).await;

        assert_eq!(result, Ok(123));
    }

    #[tokio::test]
    async fn timeout_seconds_extension() {
        let result = (async { "done" }).timeout_seconds(1).await;

        assert_eq!(result, Ok("done"));
    }

    #[tokio::test]
    async fn timeout_with_string_result() {
        let result = async { String::from("hello") }
            .timeout(Duration::from_millis(100))
            .await;

        assert_eq!(result, Ok(String::from("hello")));
    }

    #[tokio::test]
    async fn timeout_timing_accuracy() {
        let start = Instant::now();

        let result = async {
            task_utils::wait_for_millis(200).await;
            return 42;
        }
        .timeout(Duration::from_millis(100))
        .await;

        let elapsed = start.elapsed();

        assert_eq!(result, Err(TimeoutError));
        // Should time out around 100ms, not wait for full 200ms
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn timeout_with_complex_type() {
        #[derive(Debug, PartialEq)]
        struct Data {
            id: i32,
            name: String,
        }

        let result = timeout(Duration::from_millis(100), async {
            Data {
                id: 1,
                name: String::from("test"),
            }
        })
        .await;

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.id, 1);
        assert_eq!(data.name, "test");
    }

    #[tokio::test]
    async fn timeout_with_result_type() {
        let result = timeout(Duration::from_millis(100), async { Result::<i32, &str>::Ok(42) }).await;

        assert_eq!(result, Ok(Ok(42)));
    }

    #[tokio::test]
    async fn timeout_with_result_type_error() {
        let result = timeout(Duration::from_millis(100), async { Result::<i32, &str>::Err("error") }).await;

        assert_eq!(result, Ok(Err("error")));
    }

    #[tokio::test]
    async fn timeout_zero_duration() {
        let result = async {
            task_utils::wait_for_millis(10).await;
            return 42;
        }
        .timeout(Duration::from_millis(0))
        .await;

        // Should time out immediately
        assert_eq!(result, Err(TimeoutError));
    }

    #[tokio::test]
    async fn timeout_with_immediate_future() {
        let result = timeout(Duration::from_millis(100), async { 42 }).await;

        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn timeout_error_display() {
        let error = TimeoutError;
        let message = format!("{}", error);
        assert_eq!(message, "Operation timed out");
    }

    #[tokio::test]
    async fn timeout_chaining() {
        let result = (async {
            task_utils::wait_for_millis(30).await;
            return 100;
        })
        .timeout_millis(50)
        .await;

        assert_eq!(result, Ok(100));

        // Can also chain with map/and_then
        let doubled = result.map(|v| v * 2);
        assert_eq!(doubled, Ok(200));
    }

    #[tokio::test]
    async fn timeout_with_cancellation_token() {
        use crate::CancellationToken;

        let token = CancellationToken::new();
        let token_clone = token.clone();

        let result = async move {
            tokio::select! {
                _ = token_clone.cancelled_future() => "cancelled",
                _ = task_utils::wait_for_millis(300) => "completed",
            }
        }
        .timeout(Duration::from_millis(200))
        .await;

        // Should time out before completion
        assert_eq!(result, Err(TimeoutError));
    }

    #[tokio::test]
    async fn timeout_multiple_in_sequence() {
        let r1 = async { 1 }.timeout_millis(100).await;
        let r2 = async { 2 }.timeout_millis(100).await;
        let r3 = async { 3 }.timeout_millis(100).await;

        assert_eq!(r1, Ok(1));
        assert_eq!(r2, Ok(2));
        assert_eq!(r3, Ok(3));
    }

    #[tokio::test]
    async fn timeout_with_blob_task() {
        use crate::BlobTask;

        let blob = BlobTask::from_future(async {
            task_utils::wait_for_millis(50).await;
            return 42;
        });

        let result = blob.timeout_millis(100).await;
        assert_eq!(result, Ok(42));
    }

    #[tokio::test]
    async fn timeout_concurrent_operations() {
        let results = tokio::join!(
            async {
                task_utils::wait_for_millis(50).await;
                1
            }
            .timeout_millis(100),
            async {
                task_utils::wait_for_millis(50).await;
                2
            }
            .timeout_millis(100),
            async {
                task_utils::wait_for_millis(50).await;
                3
            }
            .timeout_millis(100),
        );

        assert_eq!(results.0, Ok(1));
        assert_eq!(results.1, Ok(2));
        assert_eq!(results.2, Ok(3));
    }

    #[tokio::test]
    async fn timeout_with_when_any() {
        use crate::when_any;

        let result = when_any!(
            async {
                task_utils::wait_for_millis(200).await;
                "slow"
            },
            async {
                task_utils::wait_for_millis(50).await;
                "fast"
            },
        )
        .timeout_millis(300)
        .await;

        assert!(result.is_ok());
        let when_any_result = result.unwrap();
        assert_eq!(when_any_result.result, "fast");
    }
}
