use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future that is immediately ready with a value.
/// Zero-allocation, always completes on first poll.
pub struct CompletedTask<T>(Option<T>)
where
    T: Clone + Send + 'static;

impl<T> CompletedTask<T>
where
    T: Clone + Send + 'static,
{
    /// Creates a new completed task with the given value.
    #[inline]
    pub fn new(value: T) -> Self {
        Self(Some(value))
    }
}

impl<T> Future for CompletedTask<T>
where
    T: Clone + Send + 'static,
{
    type Output = T;

    #[inline]
    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<T> {
        // SAFETY: We never move out of `self`, only take from the Option.
        // CompletedTask doesn't implement Drop or have any special drop behavior.
        let this = unsafe { self.get_unchecked_mut() };
        Poll::Ready(this.0.take().expect("CompletedTask polled after completion"))
    }
}

// Implementing Unpin since CompletedTask has no self-referential data
impl<T> Unpin for CompletedTask<T> where T: Clone + Send + 'static {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_utils;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    // Helper to create a dummy waker for testing
    fn dummy_waker() -> Waker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            raw_waker()
        }
        fn raw_waker() -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
        unsafe { Waker::from_raw(raw_waker()) }
    }

    #[test]
    fn completed_task_returns_value_immediately() {
        let mut task = CompletedTask::new(42);
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        let result = Pin::new(&mut task).poll(&mut ctx);
        assert_eq!(result, Poll::Ready(42));
    }

    #[test]
    fn completed_task_with_string() {
        let mut task = CompletedTask::new(String::from("hello"));
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        let result = Pin::new(&mut task).poll(&mut ctx);
        assert_eq!(result, Poll::Ready(String::from("hello")));
    }

    #[test]
    fn completed_task_with_unit() {
        let mut task = CompletedTask::new(());
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        let result = Pin::new(&mut task).poll(&mut ctx);
        assert_eq!(result, Poll::Ready(()));
    }

    #[test]
    fn from_result_helper() {
        let mut task = task_utils::completed_task(100);
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        let result = Pin::new(&mut task).poll(&mut ctx);
        assert_eq!(result, Poll::Ready(100));
    }

    #[test]
    #[should_panic(expected = "CompletedTask polled after completion")]
    fn completed_task_panics_on_second_poll() {
        let mut task = CompletedTask::new(42);
        let waker = dummy_waker();
        let mut ctx = Context::from_waker(&waker);

        // First poll succeeds
        let _ = Pin::new(&mut task).poll(&mut ctx);

        // Second poll should panic
        let _ = Pin::new(&mut task).poll(&mut ctx);
    }

    #[tokio::test]
    async fn completed_task_in_async_context() {
        let result = CompletedTask::new(999).await;
        assert_eq!(result, 999);
    }

    #[tokio::test]
    async fn from_result_in_async_context() {
        let result = task_utils::completed_task("test").await;
        assert_eq!(result, "test");
    }

    #[test]
    fn completed_task_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<CompletedTask<i32>>();
    }

    #[tokio::test]
    async fn completed_task_with_complex_type() {
        #[derive(Debug, PartialEq, Clone)]
        struct Complex {
            name: String,
            value: i32,
        }

        let data = Complex {
            name: String::from("test"),
            value: 42,
        };

        let result = CompletedTask::new(data).await;
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
    }

    #[tokio::test]
    async fn multiple_completed_tasks_in_sequence() {
        let a = CompletedTask::new(1).await;
        let b = CompletedTask::new(2).await;
        let c = CompletedTask::new(3).await;

        assert_eq!(a + b + c, 6);
    }

    #[test]
    fn completed_task_size() {
        use std::mem::size_of;
        // Should be small - just Option<T>
        assert_eq!(size_of::<CompletedTask<i32>>(), size_of::<Option<i32>>());
    }
}
