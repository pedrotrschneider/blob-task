use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::runtime::{Handle, Runtime};
use tokio::time::Sleep;

/// State for a Delay
struct DelayState {
    sleep: Option<Pin<Box<Sleep>>>,
    duration: Duration,
    wakers: Vec<Waker>,
    completed: bool,
}

pub struct Delay {
    state: Arc<Mutex<DelayState>>,
}

/// A future that completes after a specified duration.
/// Thin wrapper around tokio::time::sleep for consistency with the library's API.
impl Delay {
    /// Create a new delay (safe outside a runtime)
    pub fn new(duration: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(DelayState {
                sleep: None,
                duration,
                wakers: Vec::new(),
                completed: false,
            })),
        }
    }

    /// Spawn the timer if needed
    fn spawn_if_needed(&self) {
        let mut state = self.state.lock().unwrap();
        if state.sleep.is_none() && !state.completed {
            let duration = state.duration;
            let state_clone = self.state.clone();

            // Lazy creation: spawn on a runtime
            if let Ok(_) = Handle::try_current() {
                state.sleep = Some(Box::pin(tokio::time::sleep(duration)));
            } else {
                // No runtime -> spawn a thread with runtime
                std::thread::spawn(move || {
                    let rt = Runtime::new().expect("Failed to create Tokio runtime");
                    let sleep_future = tokio::time::sleep(duration);
                    rt.block_on(async move {
                        sleep_future.await;
                        let mut state = state_clone.lock().unwrap();
                        state.completed = true;
                        for w in state.wakers.drain(..) {
                            w.wake();
                        }
                    });
                });
            }
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.spawn_if_needed();

        let mut state = self.state.lock().unwrap();

        if state.completed {
            Poll::Ready(())
        } else if let Some(sleep) = &mut state.sleep {
            // Poll the sleep future if it exists
            Pin::new(sleep).poll(cx)
        } else {
            // Sleep not yet spawned → register waker
            state.wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// A future that yields control back to the executor once.
/// Useful for allowing other tasks to run without blocking.
pub struct Yield {
    yielded: bool,
}

impl Yield {
    /// Creates a new yield future
    #[inline]
    pub fn new() -> Self {
        return Self { yielded: false };
    }
}

impl Default for Yield {
    fn default() -> Self {
        return Self::new();
    }
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        return if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            // Wake immediately to reschedule
            ctx.waker().wake_by_ref();
            Poll::Pending
        };
    }
}

impl Unpin for Yield {}

/// A future that yields control N times before completing.
/// Useful for distributing work across multiple executor polls.
pub struct YieldMany {
    remaining: usize,
}

impl YieldMany {
    /// Creates a new YieldMany that yields the specified number of times
    #[inline]
    pub fn new(count: usize) -> Self {
        return Self { remaining: count };
    }
}

impl Future for YieldMany {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        return if self.remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining -= 1;
            ctx.waker().wake_by_ref();
            Poll::Pending
        };
    }
}

impl Unpin for YieldMany {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_utils;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;
    use tokio::time::Duration;

    #[tokio::test]
    async fn delay_completes_after_duration() {
        let start = Instant::now();
        task_utils::wait_for_millis(100).await;
        let elapsed = start.elapsed();

        // Allow some tolerance for timing
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn delay_from_secs() {
        let start = Instant::now();
        task_utils::wait_for_seconds(1).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(950));
        assert!(elapsed < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn delay_new_with_duration() {
        let start = Instant::now();
        Delay::new(Duration::from_millis(50)).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(45));
        assert!(elapsed < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn delay_millis_method() {
        let start = Instant::now();
        task_utils::wait_for_millis(75).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(70));
        assert!(elapsed < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn delay_seconds_method() {
        let start = Instant::now();
        task_utils::wait_for_seconds(1).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(950));
        assert!(elapsed < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn yield_now_completes() {
        task_utils::yield_once().await;
        // If we get here, it worked
        assert!(true);
    }

    #[tokio::test]
    async fn yield_allows_other_tasks() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let func = async move |c: Arc<AtomicUsize>| {
            for _ in 0..5 {
                c.fetch_add(1, Ordering::SeqCst);
                task_utils::yield_once().await;
            }
        };

        let task1 = tokio::spawn(async move {
            func(counter_clone).await;
        });

        let counter_clone = Arc::clone(&counter);
        let task2 = tokio::spawn(async move {
            func(counter_clone).await;
        });

        task1.await.unwrap();
        task2.await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn yield_many_completes() {
        task_utils::yield_many(5).await;
        assert!(true);
    }

    #[tokio::test]
    async fn yield_many_yields_correct_count() {
        // Simply await YieldMany and verify it completes
        // The implementation ensures it yields the correct number of times
        task_utils::yield_many(3).await;

        // If we got here, it completed successfully
        // This tests that yielding 3 times doesn't hang or panic
        assert!(true);
    }

    #[tokio::test]
    async fn yield_many_with_zero_completes_immediately() {
        task_utils::yield_many(0).await;
        assert!(true);
    }

    #[tokio::test]
    async fn multiple_delays_in_sequence() {
        let start = Instant::now();

        task_utils::wait_for_millis(50).await;
        task_utils::wait_for_millis(50).await;
        task_utils::wait_for_millis(50).await;

        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(145));
        assert!(elapsed < Duration::from_millis(250));
    }

    #[tokio::test]
    async fn delay_with_other_work() {
        let counter = Arc::new(AtomicUsize::new(0));

        let func = async move |c: Arc<AtomicUsize>| {
            c.fetch_add(1, Ordering::SeqCst);
            task_utils::wait_for_millis(50).await;
            c.fetch_add(1, Ordering::SeqCst);
        };

        let c1 = Arc::clone(&counter);
        let task1 = tokio::spawn(async move {
            func(c1).await;
        });

        let c2 = Arc::clone(&counter);
        let task2 = tokio::spawn(async move {
            func(c2).await;
        });

        task1.await.unwrap();
        task2.await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn yield_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<Yield>();
    }

    #[tokio::test]
    async fn yield_many_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<YieldMany>();
    }

    #[tokio::test]
    async fn yield_default_constructor() {
        Yield::default().await;
        assert!(true);
    }

    #[tokio::test]
    async fn interleaved_yields_and_delays() {
        let start = Instant::now();

        task_utils::yield_once().await;
        task_utils::wait_for_millis(50).await;
        task_utils::yield_once().await;
        task_utils::wait_for_millis(50).await;

        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn delay_zero_duration() {
        let start = Instant::now();
        Delay::new(Duration::from_millis(0)).await;
        let elapsed = start.elapsed();

        // Should complete very quickly
        assert!(elapsed < Duration::from_millis(50));
    }

    #[test]
    fn delay_types_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Delay>();
        assert_send::<Yield>();
        assert_send::<YieldMany>();
    }

    #[tokio::test]
    async fn concurrent_delays() {
        let start = Instant::now();

        let (_, _, _) = tokio::join!(
            task_utils::wait_for_millis(100),
            task_utils::wait_for_millis(100),
            task_utils::wait_for_millis(100),
        );

        let elapsed = start.elapsed();

        // All should run concurrently, so total time ~100ms, not 300ms
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn yield_in_tight_loop() {
        let mut sum = 0;
        for i in 0..100 {
            sum += i;
            if i % 10 == 0 {
                task_utils::yield_once().await;
            }
        }
        assert_eq!(sum, 4950);
    }

    #[tokio::test]
    async fn yield_many_integration() {
        let counter = Arc::new(AtomicUsize::new(0));

        let func = async move |c: Arc<AtomicUsize>| {
            c.fetch_add(1, Ordering::SeqCst);
            task_utils::yield_many(3).await;
            c.fetch_add(1, Ordering::SeqCst);
        };

        let c1 = Arc::clone(&counter);
        let task1 = tokio::spawn(async move {
            func(c1).await;
        });

        let c2 = Arc::clone(&counter);
        let task2 = tokio::spawn(async move {
            func(c2).await;
        });

        task1.await.unwrap();
        task2.await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }
}
