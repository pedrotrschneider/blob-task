// cancellation_token.rs
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Shared state for cancellation token
struct CancellationSharedState {
    cancelled: bool,
    wakers: Vec<Waker>,
    parent: Option<CancellationToken>,
}

/// A token that can be used to signal and observe cancellation across async operations.
/// Can be cloned and shared across tasks and threads.
#[derive(Clone)]
pub struct CancellationToken {
    state: Arc<Mutex<CancellationSharedState>>,
}

impl CancellationToken {
    /// Creates a new cancellation token
    pub fn new() -> Self {
        return Self {
            state: Arc::new(Mutex::new(CancellationSharedState {
                cancelled: false,
                wakers: Vec::new(),
                parent: None,
            })),
        };
    }

    /// Creates a child token that will be cancelled when this token is cancelled
    pub fn new_child(&self) -> Self {
        let child = Self {
            state: Arc::new(Mutex::new(CancellationSharedState {
                cancelled: false,
                wakers: Vec::new(),
                parent: Some(self.clone()),
            })),
        };

        // If parent is already cancelled, cancel the child immediately
        if self.is_cancelled() {
            child.cancel();
        }

        return child;
    }

    /// Cancels the token, waking all waiting futures
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap();

        if state.cancelled {
            return;
        }

        state.cancelled = true;

        // Wake all waiting tasks
        for waker in state.wakers.drain(..) {
            waker.wake();
        }
    }

    /// Returns true if the token has been cancelled
    pub fn is_cancelled(&self) -> bool {
        let state = self.state.lock().unwrap();

        if state.cancelled {
            return true;
        }

        // Check if parent is cancelled
        if let Some(ref parent) = state.parent {
            return parent.is_cancelled();
        }

        return false;
    }

    /// Returns a future that completes when the token is cancelled
    pub fn cancelled_future(&self) -> CancellationFuture {
        return CancellationFuture { token: self.clone() };
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        return Self::new();
    }
}

/// Future returned by CancellationToken::cancelled()
pub struct CancellationFuture {
    token: CancellationToken,
}

impl Future for CancellationFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.token.is_cancelled() {
            return Poll::Ready(());
        }

        let mut state = self.token.state.lock().unwrap();

        // Double-check after acquiring lock
        if state.cancelled {
            return Poll::Ready(());
        }

        // Check parent again under lock
        if let Some(ref parent) = state.parent {
            if parent.is_cancelled() {
                state.cancelled = true;
                return Poll::Ready(());
            }
        }

        // Register waker
        state.wakers.push(ctx.waker().clone());

        return Poll::Pending;
    }
}

impl Unpin for CancellationFuture {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn basic_cancellation() {
        let token = CancellationToken::new();

        assert!(!token.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_future_completes() {
        let token = CancellationToken::new();
        let future = token.cancelled_future();

        token.cancel();

        future.await;
        // If we get here, it worked
        assert!(true);
    }

    #[tokio::test]
    async fn cancelled_future_waits_for_cancellation() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            token_clone.cancelled_future().await;
        });

        // Give the task time to start waiting
        sleep(Duration::from_millis(50)).await;

        token.cancel();

        // Should complete quickly now
        timeout(Duration::from_millis(100), handle)
            .await
            .expect("Timeout waiting for cancellation")
            .expect("Task panicked");
    }

    #[tokio::test]
    async fn clone_shares_cancellation_state() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        assert!(!token.is_cancelled());
        assert!(!token_clone.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
        assert!(token_clone.is_cancelled());
    }

    #[tokio::test]
    async fn multiple_futures_notified() {
        let token = CancellationToken::new();

        let fut1 = token.cancelled_future();
        let fut2 = token.cancelled_future();
        let fut3 = token.cancelled_future();

        token.cancel();

        // All should complete
        tokio::join!(fut1, fut2, fut3);

        assert!(true);
    }

    #[tokio::test]
    async fn cancel_from_another_thread() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            token_clone.cancelled_future().await;
        });

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            token.cancel();
        });

        timeout(Duration::from_secs(1), handle)
            .await
            .expect("Timeout")
            .expect("Task failed");
    }

    #[tokio::test]
    async fn child_token_cancelled_with_parent() {
        let parent = CancellationToken::new();
        let child = parent.new_child();

        assert!(!parent.is_cancelled());
        assert!(!child.is_cancelled());

        parent.cancel();

        assert!(parent.is_cancelled());
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn child_token_independent_cancellation() {
        let parent = CancellationToken::new();
        let child = parent.new_child();

        child.cancel();

        assert!(child.is_cancelled());
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn child_token_created_after_parent_cancelled() {
        let parent = CancellationToken::new();
        parent.cancel();

        let child = parent.new_child();

        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn nested_child_tokens() {
        let parent = CancellationToken::new();
        let child1 = parent.new_child();
        let child2 = child1.new_child();
        let child3 = child2.new_child();

        assert!(!child3.is_cancelled());

        parent.cancel();

        assert!(parent.is_cancelled());
        assert!(child1.is_cancelled());
        assert!(child2.is_cancelled());
        assert!(child3.is_cancelled());
    }

    #[tokio::test]
    async fn cancellation_with_select() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = token_clone.cancelled_future() => {
                    return "cancelled";
                }
                _ = sleep(Duration::from_secs(10)) => {
                    return "timeout";
                }
            }
        });

        sleep(Duration::from_millis(50)).await;
        token.cancel();

        let result = handle.await.unwrap();
        assert_eq!(result, "cancelled");
    }

    #[tokio::test]
    async fn default_constructor() {
        let token = CancellationToken::default();

        assert!(!token.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn multiple_cancels_are_safe() {
        let token = CancellationToken::new();

        token.cancel();
        token.cancel();
        token.cancel();

        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_with_racing_tasks() {
        let token = CancellationToken::new();
        let completed = Arc::new(AtomicBool::new(false));

        let mut handles = vec![];

        for _ in 0..10 {
            let token_clone = token.clone();
            let completed_clone = Arc::clone(&completed);

            let handle = tokio::spawn(async move {
                token_clone.cancelled_future().await;
                completed_clone.store(true, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        sleep(Duration::from_millis(50)).await;
        token.cancel();

        for handle in handles {
            handle.await.unwrap();
        }

        assert!(completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancelled_future_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<CancellationFuture>();
    }

    #[test]
    fn cancellation_token_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<CancellationToken>();
        assert_sync::<CancellationToken>();
    }

    #[test]
    fn cancellation_future_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<CancellationFuture>();
    }

    #[tokio::test]
    async fn child_future_completes_on_parent_cancel() {
        let parent = CancellationToken::new();
        let child = parent.new_child();

        let child_future = child.cancelled_future();

        parent.cancel();

        child_future.await;
        assert!(true);
    }

    #[tokio::test]
    async fn practical_timeout_pattern() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let work = tokio::spawn(async move {
            tokio::select! {
                _ = token_clone.cancelled_future() => {
                    return Err("cancelled");
                }
                _ = sleep(Duration::from_millis(100)) => {
                    return Ok("completed");
                }
            }
        });

        // Cancel before work completes
        sleep(Duration::from_millis(50)).await;
        token.cancel();

        let result = work.await.unwrap();
        assert_eq!(result, Err("cancelled"));
    }
}
