use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future that completes when all provided futures complete.
/// Returns a vector of all results in order.
pub struct WhenAll<F>
where
    F: Future,
{
    futures: Vec<Pin<Box<F>>>,
    results: Vec<Option<F::Output>>,
    completed_count: usize,
}

impl<F> WhenAll<F>
where
    F: Future,
{
    /// Creates a new WhenAll combinator from a vector of futures
    pub fn new(futures: Vec<F>) -> Self {
        let len = futures.len();
        return Self {
            futures: futures.into_iter().map(Box::pin).collect(),
            results: (0..len).map(|_| None).collect(),
            completed_count: 0,
        };
    }
}

impl<F> Future for WhenAll<F>
where
    F: Future,
{
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We're not moving anything out of self, only accessing and modifying fields.
        // WhenAll doesn't have self-referential data.
        let this = unsafe { self.get_unchecked_mut() };

        let future_count = this.futures.len();

        // Handle empty case
        if future_count == 0 {
            return Poll::Ready(Vec::new());
        }

        // Poll all incomplete futures
        for i in 0..future_count {
            if this.results[i].is_some() {
                continue; // Already completed
            }

            match this.futures[i].as_mut().poll(cx) {
                Poll::Ready(result) => {
                    this.results[i] = Some(result);
                    this.completed_count += 1;
                }
                Poll::Pending => continue,
            }
        }

        // Check if all completed
        if this.completed_count == future_count {
            let results: Vec<F::Output> = this.results.iter_mut().map(|r| r.take().unwrap()).collect();
            return Poll::Ready(results);
        }

        return Poll::Pending;
    }
}

/// Result of a WhenAny operation, containing the result and its index
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenAnyResult<T> {
    pub result: T,
    pub index: usize,
}

/// Future that completes when any of the provided futures complete.
/// Returns the first result along with its index.
pub struct WhenAny<F>
where
    F: Future,
{
    futures: Vec<Pin<Box<F>>>,
}

impl<F> WhenAny<F>
where
    F: Future,
{
    /// Creates a new WhenAny combinator from a vector of futures
    pub fn new(futures: Vec<F>) -> Self {
        return Self {
            futures: futures.into_iter().map(Box::pin).collect(),
        };
    }
}

impl<F> Future for WhenAny<F>
where
    F: Future,
{
    type Output = WhenAnyResult<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We're not moving anything out of self, only accessing fields.
        // WhenAny doesn't have self-referential data.
        let this = unsafe { self.get_unchecked_mut() };

        // Handle empty case - this will never complete
        if this.futures.is_empty() {
            return Poll::Pending;
        }

        // Poll all futures, return the first one that's ready
        for (index, future) in this.futures.iter_mut().enumerate() {
            match future.as_mut().poll(cx) {
                Poll::Ready(result) => {
                    return Poll::Ready(WhenAnyResult { result, index });
                }
                Poll::Pending => {}
            }
        }

        return Poll::Pending;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completed_task::CompletedTask;
    use std::time::Duration;
    use crate::TaskUtils;

    #[tokio::test]
    async fn when_all_with_completed_tasks() {
        let futures = vec![CompletedTask::new(1), CompletedTask::new(2), CompletedTask::new(3)];

        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn when_all_empty_vec() {
        let futures: Vec<CompletedTask<i32>> = vec![];
        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results, Vec::<i32>::new());
    }

    #[tokio::test]
    async fn when_all_single_future() {
        let futures = vec![CompletedTask::new(42)];
        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results, vec![42]);
    }

    #[tokio::test]
    async fn when_all_with_delays() {
        let futures = vec![
            Box::pin(async {
                TaskUtils::wait_for_millis(50).await;
                return 1;
            }) as Pin<Box<dyn Future<Output = i32> + Send>>,
            Box::pin(async {
                TaskUtils::wait_for_millis(100).await;
                return 2;
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(25).await;
                return 3;
            }),
        ];

        let start = std::time::Instant::now();
        let results = TaskUtils::when_all(futures).await;
        let elapsed = start.elapsed();

        assert_eq!(results, vec![1, 2, 3]);
        // Should wait for the longest one (~100ms)
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn when_all_with_strings() {
        let futures = vec![
            CompletedTask::new(String::from("hello")),
            CompletedTask::new(String::from("world")),
        ];

        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results, vec![String::from("hello"), String::from("world")]);
    }

    #[tokio::test]
    async fn when_all_preserves_order() {
        let futures = vec![
            Box::pin(async {
                TaskUtils::wait_for_millis(100).await;
                return 1;
            }) as Pin<Box<dyn Future<Output = i32> + Send>>,
            Box::pin(async {
                TaskUtils::wait_for_millis(50).await;
                return 2;
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(75).await;
                return 3;
            }),
        ];

        let results = TaskUtils::when_all(futures).await;

        // Despite different completion times, order is preserved
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn when_any_with_completed_tasks() {
        let futures = vec![CompletedTask::new(1), CompletedTask::new(2), CompletedTask::new(3)];

        let result = TaskUtils::when_any(futures).await;

        // First one should complete (but any is valid)
        assert_eq!(result.index, 0);
        assert_eq!(result.result, 1);
    }

    #[tokio::test]
    async fn when_any_single_future() {
        let futures = vec![CompletedTask::new(42)];
        let result = TaskUtils::when_any(futures).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 42);
    }

    #[tokio::test]
    async fn when_any_returns_fastest() {
        let futures = vec![
            Box::pin(async {
                TaskUtils::wait_for_millis(100).await;
                return 1;
            }) as Pin<Box<dyn Future<Output = i32> + Send>>,
            Box::pin(async {
                TaskUtils::wait_for_millis(25).await;
                return 2;
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(50).await;
                return 3;
            }),
        ];

        let start = std::time::Instant::now();
        let result = TaskUtils::when_any(futures).await;
        let elapsed = start.elapsed();

        // Should return the fastest (index 1, value 2)
        assert_eq!(result.index, 1);
        assert_eq!(result.result, 2);
        // Should complete in ~25ms, not 100ms
        assert!(elapsed < Duration::from_millis(75));
    }

    #[tokio::test]
    async fn when_any_with_strings() {
        let futures = vec![
            CompletedTask::new(String::from("first")),
            CompletedTask::new(String::from("second")),
        ];

        let result = TaskUtils::when_any(futures).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, String::from("first"));
    }

    #[tokio::test]
    async fn when_all_struct_constructor() {
        let futures = vec![CompletedTask::new(10), CompletedTask::new(20)];

        let results = WhenAll::new(futures).await;

        assert_eq!(results, vec![10, 20]);
    }

    #[tokio::test]
    async fn when_any_struct_constructor() {
        let futures = vec![CompletedTask::new(99)];

        let result = WhenAny::new(futures).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 99);
    }

    #[tokio::test]
    async fn when_all_with_unit_type() {
        let futures = vec![CompletedTask::new(()), CompletedTask::new(()), CompletedTask::new(())];

        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn when_any_result_struct() {
        let result = WhenAnyResult { result: 42, index: 5 };

        assert_eq!(result.result, 42);
        assert_eq!(result.index, 5);
    }

    #[tokio::test]
    async fn when_any_result_clone() {
        let result = WhenAnyResult {
            result: String::from("test"),
            index: 2,
        };

        let cloned = result.clone();

        assert_eq!(result, cloned);
    }

    #[tokio::test]
    async fn when_all_with_complex_type() {
        #[derive(Debug, PartialEq)]
        struct Data {
            id: i32,
            name: String,
        }

        let futures = vec![
            CompletedTask::new(Data {
                id: 1,
                name: String::from("first"),
            }),
            CompletedTask::new(Data {
                id: 2,
                name: String::from("second"),
            }),
        ];

        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[1].name, "second");
    }

    #[tokio::test]
    async fn when_any_with_different_completion_times() {
        let futures = vec![
            Box::pin(async {
                TaskUtils::wait_for_millis(200).await;
                return "slow";
            }) as Pin<Box<dyn Future<Output = &str> + Send>>,
            Box::pin(async {
                TaskUtils::wait_for_millis(50).await;
                return "fast";
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(300).await;
                return "slowest";
            }),
        ];

        let result = TaskUtils::when_any(futures).await;

        assert_eq!(result.result, "fast");
        assert_eq!(result.index, 1);
    }

    #[tokio::test]
    async fn when_all_large_number_of_futures() {
        let futures: Vec<_> = (0..100).map(|i| CompletedTask::new(i)).collect();

        let results = TaskUtils::when_all(futures).await;

        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 0);
        assert_eq!(results[99], 99);
    }

    #[tokio::test]
    async fn when_any_large_number_of_futures() {
        let futures: Vec<_> = (0..100).map(|i| CompletedTask::new(i)).collect();

        let result = TaskUtils::when_any(futures).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 0);
    }

    #[tokio::test]
    async fn when_all_mixed_delay_times() {
        let futures = vec![
            Box::pin(async {
                TaskUtils::wait_for_millis(30).await;
                return 1;
            }) as Pin<Box<dyn Future<Output = i32> + Send>>,
            Box::pin(async {
                TaskUtils::wait_for_millis(60).await;
                return 2;
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(45).await;
                return 3;
            }),
            Box::pin(async {
                TaskUtils::wait_for_millis(15).await;
                return 4;
            }),
        ];

        let results = TaskUtils::when_all(futures).await;

        // Order preserved despite different times
        assert_eq!(results, vec![1, 2, 3, 4]);
    }
}
