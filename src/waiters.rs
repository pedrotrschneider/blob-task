use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// A future that completes when a predicate function returns true.
/// Similar to UniTask's WaitUntil.
///
/// Polls the predicate on each poll until it returns true.
pub struct WaitUntil<F>
where
    F: Fn() -> bool,
{
    predicate: Arc<F>,
}

impl<F> WaitUntil<F>
where
    F: Fn() -> bool,
{
    /// Creates a new WaitUntil future with the given predicate
    #[inline]
    pub fn new(predicate: F) -> Self {
        return Self {
            predicate: Arc::new(predicate),
        };
    }
}

impl<F> Future for WaitUntil<F>
where
    F: Fn() -> bool,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        return if (self.predicate)() {
            Poll::Ready(())
        } else {
            // Wake immediately to check again on next poll
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<F> Clone for WaitUntil<F>
where
    F: Fn() -> bool,
{
    fn clone(&self) -> Self {
        return Self {
            predicate: Arc::clone(&self.predicate),
        };
    }
}

impl<F> Unpin for WaitUntil<F> where F: Fn() -> bool {}

/// A future that completes when a predicate function returns false.
/// Similar to UniTask's WaitWhile.
///
/// Polls the predicate on each poll until it returns false.
pub struct WaitWhile<F>
where
    F: Fn() -> bool,
{
    predicate: Arc<F>,
}

impl<F> WaitWhile<F>
where
    F: Fn() -> bool,
{
    /// Creates a new WaitWhile future with the given predicate
    #[inline]
    pub fn new(predicate: F) -> Self {
        return Self {
            predicate: Arc::new(predicate),
        };
    }
}

impl<F> Future for WaitWhile<F>
where
    F: Fn() -> bool,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        return if !(self.predicate)() {
            Poll::Ready(())
        } else {
            // Wake immediately to check again on next poll
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<F> Clone for WaitWhile<F>
where
    F: Fn() -> bool,
{
    fn clone(&self) -> Self {
        return Self {
            predicate: Arc::clone(&self.predicate),
        };
    }
}

impl<F> Unpin for WaitWhile<F> where F: Fn() -> bool {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn wait_until_immediate_true() {
        let wait = crate::wait_until(|| true);
        wait.await;
        assert!(true);
    }

    #[tokio::test]
    async fn wait_until_delayed_condition() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_until(move || flag_clone.load(Ordering::SeqCst));

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(true, Ordering::SeqCst);
        });

        wait.await;
        assert!(flag.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn wait_until_with_counter() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let wait = crate::wait_until(move || counter_clone.load(Ordering::SeqCst) >= 5);

        let counter_incrementer = Arc::clone(&counter);
        tokio::spawn(async move {
            for _ in 0..5 {
                sleep(Duration::from_millis(10)).await;
                counter_incrementer.fetch_add(1, Ordering::SeqCst);
            }
        });

        wait.await;
        assert!(counter.load(Ordering::SeqCst) >= 5);
    }

    #[tokio::test]
    async fn wait_while_immediate_false() {
        let wait = crate::wait_while(|| false);
        wait.await;
        assert!(true);
    }

    #[tokio::test]
    async fn wait_while_delayed_condition() {
        let flag = Arc::new(AtomicBool::new(true));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_while(move || flag_clone.load(Ordering::SeqCst));

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(false, Ordering::SeqCst);
        });

        wait.await;
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn wait_while_with_counter() {
        let counter = Arc::new(AtomicUsize::new(10));
        let counter_clone = Arc::clone(&counter);

        let wait = crate::wait_while(move || counter_clone.load(Ordering::SeqCst) > 0);

        let counter_decrementer = Arc::clone(&counter);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_millis(10)).await;
                let val = counter_decrementer.fetch_sub(1, Ordering::SeqCst);
                if val == 1 {
                    break;
                }
            }
        });

        wait.await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn wait_until_clone() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_until(move || flag_clone.load(Ordering::SeqCst));
        let wait_clone = wait.clone();

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(true, Ordering::SeqCst);
        });

        wait_clone.await;
        assert!(true);
    }

    #[tokio::test]
    async fn wait_while_clone() {
        let flag = Arc::new(AtomicBool::new(true));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_while(move || flag_clone.load(Ordering::SeqCst));
        let wait_clone = wait.clone();

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(false, Ordering::SeqCst);
        });

        wait_clone.await;
        assert!(true);
    }

    #[tokio::test]
    async fn wait_until_multiple_checks() {
        let counter = Arc::new(AtomicUsize::new(0));
        let check_count = Arc::new(AtomicUsize::new(0));

        let counter_clone = Arc::clone(&counter);
        let check_count_clone = Arc::clone(&check_count);

        let wait = crate::wait_until(move || {
            check_count_clone.fetch_add(1, Ordering::SeqCst);
            counter_clone.load(Ordering::SeqCst) >= 3
        });

        let counter_incrementer = Arc::clone(&counter);
        tokio::spawn(async move {
            for _ in 0..3 {
                sleep(Duration::from_millis(20)).await;
                counter_incrementer.fetch_add(1, Ordering::SeqCst);
            }
        });

        wait.await;

        // Should have checked multiple times
        assert!(check_count.load(Ordering::SeqCst) > 1);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn wait_while_multiple_checks() {
        let counter = Arc::new(AtomicUsize::new(5));
        let check_count = Arc::new(AtomicUsize::new(0));

        let counter_clone = Arc::clone(&counter);
        let check_count_clone = Arc::clone(&check_count);

        let wait = crate::wait_while(move || {
            check_count_clone.fetch_add(1, Ordering::SeqCst);
            counter_clone.load(Ordering::SeqCst) > 0
        });

        let counter_decrementer = Arc::clone(&counter);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_millis(20)).await;
                let val = counter_decrementer.fetch_sub(1, Ordering::SeqCst);
                if val == 1 {
                    break;
                }
            }
        });

        wait.await;

        // Should have checked multiple times
        assert!(check_count.load(Ordering::SeqCst) > 1);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn wait_until_with_complex_condition() {
        let value1 = Arc::new(AtomicUsize::new(0));
        let value2 = Arc::new(AtomicUsize::new(0));

        let v1_clone = Arc::clone(&value1);
        let v2_clone = Arc::clone(&value2);

        let wait = crate::wait_until(move || {
            let v1 = v1_clone.load(Ordering::SeqCst);
            let v2 = v2_clone.load(Ordering::SeqCst);
            v1 + v2 >= 10
        });

        let v1_inc = Arc::clone(&value1);
        let v2_inc = Arc::clone(&value2);

        tokio::spawn(async move {
            for _ in 0..7 {
                sleep(Duration::from_millis(10)).await;
                v1_inc.fetch_add(1, Ordering::SeqCst);
            }
        });

        tokio::spawn(async move {
            for _ in 0..5 {
                sleep(Duration::from_millis(15)).await;
                v2_inc.fetch_add(1, Ordering::SeqCst);
            }
        });

        wait.await;

        let sum = value1.load(Ordering::SeqCst) + value2.load(Ordering::SeqCst);
        assert!(sum >= 10);
    }

    #[tokio::test]
    async fn wait_until_with_select() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_until(move || flag_clone.load(Ordering::SeqCst));

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(true, Ordering::SeqCst);
        });

        let result = tokio::select! {
            _ = wait => "wait_until",
            _ = sleep(Duration::from_secs(1)) => "timeout",
        };

        assert_eq!(result, "wait_until");
    }

    #[tokio::test]
    async fn wait_while_with_select() {
        let flag = Arc::new(AtomicBool::new(true));
        let flag_clone = Arc::clone(&flag);

        let wait = crate::wait_while(move || flag_clone.load(Ordering::SeqCst));

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(false, Ordering::SeqCst);
        });

        let result = tokio::select! {
            _ = wait => "wait_while",
            _ = sleep(Duration::from_secs(1)) => "timeout",
        };

        assert_eq!(result, "wait_while");
    }

    #[test]
    fn wait_until_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<WaitUntil<fn() -> bool>>();
    }

    #[test]
    fn wait_while_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<WaitWhile<fn() -> bool>>();
    }

    #[test]
    fn wait_until_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<WaitUntil<fn() -> bool>>();
    }

    #[test]
    fn wait_while_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<WaitWhile<fn() -> bool>>();
    }

    #[tokio::test]
    async fn wait_until_concurrent_waiters() {
        let flag = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        for _ in 0..5 {
            let flag_clone = Arc::clone(&flag);
            let counter_clone = Arc::clone(&counter);

            let handle = tokio::spawn(async move {
                let wait = crate::wait_until(move || flag_clone.load(Ordering::SeqCst));
                wait.await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        sleep(Duration::from_millis(50)).await;
        flag.store(true, Ordering::SeqCst);

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn wait_while_concurrent_waiters() {
        let flag = Arc::new(AtomicBool::new(true));
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        for _ in 0..5 {
            let flag_clone = Arc::clone(&flag);
            let counter_clone = Arc::clone(&counter);

            let handle = tokio::spawn(async move {
                let wait = crate::wait_while(move || flag_clone.load(Ordering::SeqCst));
                wait.await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        sleep(Duration::from_millis(50)).await;
        flag.store(false, Ordering::SeqCst);

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
}