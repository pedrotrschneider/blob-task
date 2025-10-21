use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// A future that completes when a value changes from its initial state.
/// Similar to UniTask's WaitForValueChanged.
///
/// Polls a getter function until the value differs from the initial value.
pub struct WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    getter: Arc<F>,
    initial_value: T,
}

impl<F, T> WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    /// Creates a new WaitForValueChanged future.
    ///
    /// # Arguments
    /// * `getter` - Function that retrieves the current value
    /// * `initial_value` - The value to compare against
    #[inline]
    pub fn new(getter: F, initial_value: T) -> Self {
        return Self {
            getter: Arc::new(getter),
            initial_value,
        };
    }
}

impl<F, T> Future for WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_value = (self.getter)();

        return if current_value != self.initial_value {
            Poll::Ready(current_value)
        } else {
            // Wake immediately to check again on next poll
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<F, T> Clone for WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq + Clone,
{
    fn clone(&self) -> Self {
        return Self {
            getter: Arc::clone(&self.getter),
            initial_value: self.initial_value.clone(),
        };
    }
}

impl<F, T> Unpin for WaitForValueChanged<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
}

/// A future that waits for a value to change to a specific target value.
/// More specific than WaitForValueChanged - waits for an exact match.
pub struct WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    getter: Arc<F>,
    target_value: T,
}

impl<F, T> WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    /// Creates a new WaitForValueEquals future.
    ///
    /// # Arguments
    /// * `getter` - Function that retrieves the current value
    /// * `target_value` - The value to wait for
    #[inline]
    pub fn new(getter: F, target_value: T) -> Self {
        return Self {
            getter: Arc::new(getter),
            target_value,
        };
    }
}

impl<F, T> Future for WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_value = (self.getter)();

        return if current_value == self.target_value {
            Poll::Ready(())
        } else {
            // Wake immediately to check again on next poll
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<F, T> Clone for WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq + Clone,
{
    fn clone(&self) -> Self {
        return Self {
            getter: Arc::clone(&self.getter),
            target_value: self.target_value.clone(),
        };
    }
}

impl<F, T> Unpin for WaitForValueEquals<F, T>
where
    F: Fn() -> T,
    T: PartialEq,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn wait_value_changed_immediate() {
        let value = Arc::new(AtomicUsize::new(10));
        let initial = value.load(Ordering::SeqCst);

        // Change value before waiting
        value.store(20, Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let result = wait.await;
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn wait_value_changed_delayed() {
        let value = Arc::new(AtomicUsize::new(5));
        let initial = value.load(Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(15, Ordering::SeqCst);
        });

        let result = wait.await;
        assert_eq!(result, 15);
    }

    #[tokio::test]
    async fn wait_value_changed_with_bool() {
        let flag = Arc::new(AtomicBool::new(false));
        let initial = flag.load(Ordering::SeqCst);

        let flag_clone = Arc::clone(&flag);
        let wait = WaitForValueChanged::new(
            move || flag_clone.load(Ordering::SeqCst),
            initial,
        );

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(true, Ordering::SeqCst);
        });

        let result = wait.await;
        assert_eq!(result, true);
    }

    #[tokio::test]
    async fn wait_value_changed_with_string() {
        let value = Arc::new(Mutex::new(String::from("initial")));
        let initial = value.lock().unwrap().clone();

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.lock().unwrap().clone(),
            initial,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            *value_setter.lock().unwrap() = String::from("changed");
        });

        let result = wait.await;
        assert_eq!(result, "changed");
    }

    #[tokio::test]
    async fn wait_value_changed_multiple_changes() {
        let value = Arc::new(AtomicUsize::new(0));
        let initial = value.load(Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            for i in 1..=10 {
                sleep(Duration::from_millis(10)).await;
                value_setter.store(i, Ordering::SeqCst);
            }
        });

        let result = wait.await;
        // Should detect the first change
        assert_ne!(result, 0);
    }

    #[tokio::test]
    async fn wait_value_equals_immediate() {
        let value = Arc::new(AtomicUsize::new(42));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.load(Ordering::SeqCst),
            42,
        );

        wait.await;
        assert!(true);
    }

    #[tokio::test]
    async fn wait_value_equals_delayed() {
        let value = Arc::new(AtomicUsize::new(0));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.load(Ordering::SeqCst),
            100,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(100, Ordering::SeqCst);
        });

        wait.await;
        assert_eq!(value.load(Ordering::SeqCst), 100);
    }

    #[tokio::test]
    async fn wait_value_equals_with_bool() {
        let flag = Arc::new(AtomicBool::new(false));

        let flag_clone = Arc::clone(&flag);
        let wait = WaitForValueEquals::new(
            move || flag_clone.load(Ordering::SeqCst),
            true,
        );

        let flag_setter = Arc::clone(&flag);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            flag_setter.store(true, Ordering::SeqCst);
        });

        wait.await;
        assert!(flag.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn wait_value_equals_with_string() {
        let value = Arc::new(Mutex::new(String::from("waiting")));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.lock().unwrap().clone(),
            String::from("ready"),
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            *value_setter.lock().unwrap() = String::from("ready");
        });

        wait.await;
        assert_eq!(*value.lock().unwrap(), "ready");
    }

    #[tokio::test]
    async fn wait_value_changed_clone() {
        let value = Arc::new(AtomicUsize::new(10));
        let initial = value.load(Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let wait_clone = wait.clone();

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(20, Ordering::SeqCst);
        });

        let result = wait_clone.await;
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn wait_value_equals_clone() {
        let value = Arc::new(AtomicUsize::new(0));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.load(Ordering::SeqCst),
            50,
        );

        let wait_clone = wait.clone();

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(50, Ordering::SeqCst);
        });

        wait_clone.await;
        assert_eq!(value.load(Ordering::SeqCst), 50);
    }

    #[tokio::test]
    async fn wait_value_changed_with_select() {
        let value = Arc::new(AtomicUsize::new(0));
        let initial = value.load(Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(99, Ordering::SeqCst);
        });

        let result = tokio::select! {
            v = wait => v,
            _ = sleep(Duration::from_secs(1)) => 0,
        };

        assert_eq!(result, 99);
    }

    #[tokio::test]
    async fn wait_value_equals_with_select() {
        let value = Arc::new(AtomicUsize::new(0));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.load(Ordering::SeqCst),
            77,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(77, Ordering::SeqCst);
        });

        let result = tokio::select! {
            _ = wait => "changed",
            _ = sleep(Duration::from_secs(1)) => "timeout",
        };

        assert_eq!(result, "changed");
    }

    #[test]
    fn wait_value_changed_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<WaitForValueChanged<fn() -> i32, i32>>();
    }

    #[test]
    fn wait_value_equals_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<WaitForValueEquals<fn() -> i32, i32>>();
    }

    #[tokio::test]
    async fn wait_value_changed_with_complex_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct State {
            status: String,
            count: usize,
        }

        let state = Arc::new(Mutex::new(State {
            status: String::from("init"),
            count: 0,
        }));

        let initial = state.lock().unwrap().clone();

        let state_clone = Arc::clone(&state);
        let wait = WaitForValueChanged::new(
            move || state_clone.lock().unwrap().clone(),
            initial,
        );

        let state_setter = Arc::clone(&state);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            *state_setter.lock().unwrap() = State {
                status: String::from("running"),
                count: 5,
            };
        });

        let result = wait.await;
        assert_eq!(result.status, "running");
        assert_eq!(result.count, 5);
    }

    #[tokio::test]
    async fn wait_value_changed_incremental() {
        let counter = Arc::new(AtomicUsize::new(0));
        let initial = counter.load(Ordering::SeqCst);

        let counter_clone = Arc::clone(&counter);
        let wait = WaitForValueChanged::new(
            move || counter_clone.load(Ordering::SeqCst),
            initial,
        );

        let counter_incrementer = Arc::clone(&counter);
        tokio::spawn(async move {
            for i in 1..=5 {
                sleep(Duration::from_millis(20)).await;
                counter_incrementer.store(i, Ordering::SeqCst);
            }
        });

        let result = wait.await;
        // Should get the first change (1 or later if timing varies)
        assert!(result >= 1);
    }

    #[tokio::test]
    async fn wait_value_equals_skip_intermediate_values() {
        let value = Arc::new(AtomicUsize::new(0));

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueEquals::new(
            move || value_clone.load(Ordering::SeqCst),
            10,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            for i in 1..=10 {
                sleep(Duration::from_millis(10)).await;
                value_setter.store(i, Ordering::SeqCst);
            }
        });

        wait.await;
        assert_eq!(value.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn wait_value_changed_concurrent_waiters() {
        let value = Arc::new(AtomicUsize::new(100));
        let initial = value.load(Ordering::SeqCst);
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        for _ in 0..5 {
            let value_clone = Arc::clone(&value);
            let counter_clone = Arc::clone(&counter);

            let handle = tokio::spawn(async move {
                let wait = WaitForValueChanged::new(
                    move || value_clone.load(Ordering::SeqCst),
                    initial,
                );
                wait.await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        sleep(Duration::from_millis(50)).await;
        value.store(200, Ordering::SeqCst);

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn wait_value_equals_concurrent_waiters() {
        let value = Arc::new(AtomicUsize::new(0));
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        for _ in 0..5 {
            let value_clone = Arc::clone(&value);
            let counter_clone = Arc::clone(&counter);

            let handle = tokio::spawn(async move {
                let wait = WaitForValueEquals::new(
                    move || value_clone.load(Ordering::SeqCst),
                    42,
                );
                wait.await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        sleep(Duration::from_millis(50)).await;
        value.store(42, Ordering::SeqCst);

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn wait_value_changed_with_timeout() {
        use crate::timeout::TimeoutExt;

        let value = Arc::new(AtomicUsize::new(0));
        let initial = value.load(Ordering::SeqCst);

        let value_clone = Arc::clone(&value);
        let wait = WaitForValueChanged::new(
            move || value_clone.load(Ordering::SeqCst),
            initial,
        );

        let value_setter = Arc::clone(&value);
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            value_setter.store(99, Ordering::SeqCst);
        });

        let result = wait.timeout_millis(200).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 99);
    }
}
