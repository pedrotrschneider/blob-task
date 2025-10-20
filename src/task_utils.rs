use std::time::Duration;
use crate::{CancellationToken, CompletedTask, Delay, TaskCompletionSource, WhenAll, WhenAny, Yield, YieldMany};

pub struct TaskUtils {}

impl TaskUtils {
    /// complete_task.rs utils

    #[inline]
    pub fn completed_task<T>(value: T) -> CompletedTask<T> {
        return CompletedTask::new(value);
    }

    /// delay_yield.rs utils

    /// Helper function to create delays
    #[inline]
    pub fn delay(duration: Duration) -> Delay {
        return Delay::new(duration);
    }

    /// Helper function to create a delay from milliseconds
    #[inline]
    pub fn wait_for_millis(millis: u64) -> Delay {
        return Delay::new(Duration::from_millis(millis));
    }

    /// Helper function to create a delay from seconds
    #[inline]
    pub fn wait_for_seconds(secs: u64) -> Delay {
        return Delay::new(Duration::from_secs(secs));
    }

    /// Helper function to create a yield
    #[inline]
    pub fn yield_once() -> Yield {
        return Yield::new();
    }

    /// Helper function to create a yield many
    #[inline]
    pub fn yield_many(count: usize) -> YieldMany {
        return YieldMany::new(count);
    }

    /// when_combinators.rs utils

    /// Helper function to create a WhenAll combinator
    #[inline]
    pub fn when_all<F>(futures: Vec<F>) -> WhenAll<F>
    where
        F: Future,
    {
        return WhenAll::new(futures);
    }

    /// Helper function to create a WhenAny combinator
    #[inline]
    pub fn when_any<F>(futures: Vec<F>) -> WhenAny<F>
    where
        F: Future,
    {
        return WhenAny::new(futures);
    }

    /// cancellation_token.rs utils

    #[inline]
    pub fn cancellation_token() -> CancellationToken {
        return CancellationToken::new();
    }

    /// task_completion_source.rs utils

    #[inline]
    pub fn task_completion_source<T>() -> TaskCompletionSource<T> {
        return TaskCompletionSource::new();
    }
}
