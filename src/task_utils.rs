use crate::wait_until_while::{WaitUntil, WaitWhile};
use crate::{CancellationToken, CompletedTask, Delay, TaskCompletionSource, WaitForValueChanged, WaitForValueEquals, Yield, YieldMany};
use std::time::Duration;

#[inline]
pub fn completed_task<T>(value: T) -> CompletedTask<T>
where
    T: Clone + Send + 'static,
{
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

/// Helper macro to create a WhenAll combinator
#[macro_export]
macro_rules! when_all {
    ($($fut:expr),* $(,)?) => {
        WhenAll::from_blob_tasks(vec![
            $(
                $fut.to_blob_task()
            ),*
        ])
    };
}

/// Helper macro to create a WhenAny combinator
#[macro_export]
macro_rules! when_any {
    ($($fut:expr),* $(,)?) => {
        WhenAny::from_blob_tasks(vec![
            $(
                $fut.to_blob_task()
            ),*
        ])
    };
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

/// wait_until_while utils

#[inline]
pub fn wait_until<F>(predicate: F) -> WaitUntil<F>
where
    F: Fn() -> bool,
{
    return WaitUntil::new(predicate);
}

#[inline]
pub fn wait_while<F>(predicate: F) -> WaitWhile<F>
where
    F: Fn() -> bool,
{
    return WaitWhile::new(predicate);
}

/// wait_value_changed.rs utils

/// Helper function to wait for a value to change from its initial state
#[inline]
pub fn wait_for_value_changed<F, T>(getter: F, initial_value: T) -> WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    return WaitForValueChanged::new(getter, initial_value);
}

/// Helper function to wait for a value to equal a specific target
#[inline]
pub fn wait_for_value_equals<F, T>(getter: F, target_value: T) -> WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    return WaitForValueEquals::new(getter, target_value);
}
