use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Shared state between TaskCompletionSource and its future
struct CompletionSharedState<T> {
    result: Option<T>,
    waker: Option<Waker>,
    completed: bool,
}

/// A manually-controlled future completion source.
/// Similar to UniTask's UniTaskCompletionSource.
///
/// Allows you to create a future and complete it from outside
/// the async context by calling `set_result()` or `set_exception()`.
#[derive(Clone)]
pub struct TaskCompletionSource<T> {
    state: Arc<Mutex<CompletionSharedState<T>>>,
}

impl<T> TaskCompletionSource<T> {
    /// Creates a new TaskCompletionSource
    #[inline]
    pub fn new() -> Self {
        return Self {
            state: Arc::new(Mutex::new(CompletionSharedState {
                result: None,
                waker: None,
                completed: false,
            })),
        };
    }

    /// Gets the future associated with this completion source
    #[inline]
    pub fn task(&self) -> TaskCompletionFuture<T> {
        return TaskCompletionFuture {
            state: self.state.clone(),
        };
    }

    /// Completes the task with a successful result
    /// Returns true if the task was completed, false if already completed
    pub fn complete(&self, value: T) -> bool {
        let mut state = self.state.lock().unwrap();

        if state.completed {
            return false;
        }

        state.result = Some(value);
        state.completed = true;

        // Wake the waiting future if it exists
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }

        return true;
    }

    /// Checks if the task has been completed
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.state.lock().unwrap().completed
    }

    /// Attempts to get the result if completed (non-consuming)
    pub fn try_get_result(&self) -> Option<T>
    where
        T: Clone,
    {
        let state = self.state.lock().unwrap();
        return if state.completed { state.result.clone() } else { None };
    }
}

impl<T> Default for TaskCompletionSource<T> {
    fn default() -> Self {
        return Self::new();
    }
}

/// The future returned by TaskCompletionSource::task()
pub struct TaskCompletionFuture<T> {
    state: Arc<Mutex<CompletionSharedState<T>>>,
}

impl<T> Future for TaskCompletionFuture<T>
where
    T: Clone,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        return if state.completed {
            // Clone the result instead of taking it, so multiple futures can access it
            Poll::Ready(state.result.clone().expect("Result already taken"))
        } else {
            // Store the waker for later notification
            state.waker = Some(ctx.waker().clone());
            Poll::Pending
        };
    }
}

impl<T> Unpin for TaskCompletionFuture<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn basic_completion() {
        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        // Complete it immediately
        assert!(tcs.complete(42));

        let result = task.await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn completion_from_another_thread() {
        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        let tcs_clone = tcs.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            tcs_clone.complete(99);
        });

        let result = task.await;
        assert_eq!(result, 99);
    }

    #[tokio::test]
    async fn completion_before_await() {
        let tcs = TaskCompletionSource::new();

        // Complete before creating the future
        tcs.complete(123);

        // Should still work
        let result = tcs.task().await;
        assert_eq!(result, 123);
    }

    #[tokio::test]
    async fn multiple_tasks_same_source() {
        let tcs = TaskCompletionSource::new();
        let task1 = tcs.task();
        let task2 = tcs.task();

        tcs.complete(50);

        let (r1, r2) = tokio::join!(task1, task2);
        assert_eq!(r1, 50);
        assert_eq!(r2, 50);
    }

    #[tokio::test]
    async fn set_result_returns_false_on_second_call() {
        let tcs = TaskCompletionSource::new();

        assert!(tcs.complete(1));
        assert!(!tcs.complete(2)); // Should return false

        let result = tcs.task().await;
        assert_eq!(result, 1); // First value should be used
    }

    #[tokio::test]
    async fn is_completed_check() {
        let tcs = TaskCompletionSource::new();

        assert!(!tcs.is_completed());

        tcs.complete(42);

        assert!(tcs.is_completed());
    }

    #[tokio::test]
    async fn try_get_result_before_completion() {
        let tcs: TaskCompletionSource<i32> = TaskCompletionSource::new();

        assert_eq!(tcs.try_get_result(), None);

        tcs.complete(42);

        assert_eq!(tcs.try_get_result(), Some(42));
    }

    #[tokio::test]
    async fn completion_with_string() {
        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            tcs.complete(String::from("hello"));
        });

        let result = task.await;
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn completion_with_complex_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct Data {
            id: u32,
            name: String,
        }

        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        let data = Data {
            id: 1,
            name: String::from("test"),
        };

        tcs.complete(data.clone());

        let result = task.await;
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn clone_and_complete() {
        let tcs = TaskCompletionSource::new();
        let tcs_clone = tcs.clone();
        let task = tcs.task();

        // Complete from clone
        tcs_clone.complete(777);

        let result = task.await;
        assert_eq!(result, 777);
    }

    #[tokio::test]
    async fn concurrent_completions_from_multiple_threads() {
        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        // Spawn multiple threads trying to complete
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let tcs_clone = tcs.clone();
                thread::spawn(move || tcs_clone.complete(i))
            })
            .collect();

        // Wait for all threads
        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Exactly one should succeed
        assert_eq!(results.iter().filter(|&&x| x).count(), 1);

        // Task should complete with one of the values
        let result = task.await;
        assert!(result < 10);
    }

    #[tokio::test]
    async fn default_constructor() {
        let tcs: TaskCompletionSource<i32> = TaskCompletionSource::default();
        let task = tcs.task();

        tcs.complete(42);

        let result = task.await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn task_completion_with_unit_type() {
        let tcs = TaskCompletionSource::new();
        let task = tcs.task();

        tcs.complete(());

        task.await;
        // If we get here, it worked
        assert!(true);
    }

    #[tokio::test]
    async fn sequential_task_creations() {
        let tcs = TaskCompletionSource::new();
        println!("Hello before set result!");
        tcs.complete(100);

        // Create multiple tasks sequentially after completion
        println!("Hello before r1!");
        let r1 = tcs.task().await;
        println!("Hello before r2!");
        let r2 = tcs.task().await;
        println!("Hello before r3!");
        let r3 = tcs.task().await;
        println!("Hello after all tasks!");

        assert_eq!(r1, 100);
        assert_eq!(r2, 100);
        assert_eq!(r3, 100);
    }

    #[test]
    fn task_completion_source_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<TaskCompletionSource<i32>>();
        assert_sync::<TaskCompletionSource<i32>>();
    }

    #[test]
    fn task_completion_future_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TaskCompletionFuture<i32>>();
    }
}
