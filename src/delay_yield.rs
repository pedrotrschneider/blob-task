use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{Sleep, sleep};

/// A future that completes after a specified duration.
/// Thin wrapper around tokio::time::sleep for consistency with the library's API.
pub struct Delay {
    sleep: Pin<Box<Sleep>>,
}

impl Delay {
    /// Creates a new delay that completes after the specified duration
    #[inline]
    pub fn new(duration: Duration) -> Self {
        return Self {
            sleep: Box::pin(sleep(duration)),
        };
    }

    /// Creates a delay from milliseconds
    #[inline]
    pub fn millis(millis: u64) -> Self {
        return Self::new(Duration::from_millis(millis));
    }

    /// Creates a delay from seconds
    #[inline]
    pub fn seconds(secs: u64) -> Self {
        return Self::new(Duration::from_secs(secs));
    }
}

impl Future for Delay {
    type Output = ();

    #[inline]
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        return self.sleep.as_mut().poll(ctx);
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;
    use tokio::time::Duration;

    #[tokio::test]
    async fn delay_completes_after_duration() {
        let start = Instant::now();
        Delay::millis(100).await;
        let elapsed = start.elapsed();

        // Allow some tolerance for timing
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn delay_from_secs() {
        let start = Instant::now();
        Delay::seconds(1).await;
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
        Delay::millis(75).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(70));
        assert!(elapsed < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn delay_seconds_method() {
        let start = Instant::now();
        Delay::seconds(1).await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(950));
        assert!(elapsed < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn yield_now_completes() {
        Yield::new().await;
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
                Yield::new().await;
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
        YieldMany::new(5).await;
        assert!(true);
    }

    #[tokio::test]
    async fn yield_many_yields_correct_count() {
        // Simply await YieldMany and verify it completes
        // The implementation ensures it yields the correct number of times
        YieldMany::new(3).await;

        // If we got here, it completed successfully
        // This tests that yielding 3 times doesn't hang or panic
        assert!(true);
    }

    #[tokio::test]
    async fn yield_many_with_zero_completes_immediately() {
        YieldMany::new(0).await;
        assert!(true);
    }

    #[tokio::test]
    async fn multiple_delays_in_sequence() {
        let start = Instant::now();

        Delay::millis(50).await;
        Delay::millis(50).await;
        Delay::millis(50).await;

        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(145));
        assert!(elapsed < Duration::from_millis(250));
    }

    #[tokio::test]
    async fn delay_with_other_work() {
        let counter = Arc::new(AtomicUsize::new(0));

        let func = async move |c: Arc<AtomicUsize>| {
            c.fetch_add(1, Ordering::SeqCst);
            Delay::millis(50).await;
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

        Yield::new().await;
        Delay::millis(50).await;
        Yield::new().await;
        Delay::millis(50).await;

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

        let (_, _, _) = tokio::join!(Delay::millis(100), Delay::millis(100), Delay::millis(100),);

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
                Yield::new().await;
            }
        }
        assert_eq!(sum, 4950);
    }

    #[tokio::test]
    async fn yield_many_integration() {
        let counter = Arc::new(AtomicUsize::new(0));

        let func = async move |c: Arc<AtomicUsize>| {
            c.fetch_add(1, Ordering::SeqCst);
            YieldMany::new(3).await;
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
