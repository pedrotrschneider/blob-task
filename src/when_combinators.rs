use crate::BlobTask;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future that completes when all provided futures complete.
/// Returns a vector of all results in order.
pub struct WhenAll<T>
where
    T: Clone + Send + 'static,
{
    tasks: Vec<BlobTask<T>>,
    results: Vec<Option<T>>,
    completed_count: usize,
}

impl<T> WhenAll<T>
where
    T: Clone + Send + 'static,
{
    /// Creates a new WhenAll combinator from a vector of futures
    pub fn from_futures(futures: Vec<Pin<Box<dyn Future<Output = T> + Send>>>) -> Self {
        let len = futures.len();
        return Self {
            tasks: futures.into_iter().map(BlobTask::from_future).collect(),
            results: (0..len).map(|_| None).collect(),
            completed_count: 0,
        };
    }

    pub fn from_blob_tasks(tasks: Vec<BlobTask<T>>) -> Self {
        let len = tasks.len();
        return Self {
            tasks,
            results: (0..len).map(|_| None).collect(),
            completed_count: 0,
        };
    }
}

impl<T> Future for WhenAll<T>
where
    T: Clone + Send + 'static,
{
    type Output = Vec<T>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We never move the fields, only modify them in-place
        let this = unsafe { self.get_unchecked_mut() };

        if this.tasks.is_empty() {
            return Poll::Ready(Vec::new());
        }

        for i in 0..this.tasks.len() {
            if this.results[i].is_some() {
                continue; // Already completed
            }

            let task = unsafe { Pin::new_unchecked(&mut this.tasks[i]) };
            match task.poll(ctx) {
                Poll::Ready(result) => {
                    this.results[i] = Some(result);
                    this.completed_count += 1;
                }
                Poll::Pending => {} // do nothing
            }
        }

        return if this.completed_count == this.tasks.len() {
            let results = this.results.iter_mut().map(|r| r.take().unwrap()).collect();
            Poll::Ready(results)
        } else {
            Poll::Pending
        };
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
pub struct WhenAny<T>
where
    T: Clone + Send + 'static,
{
    tasks: Vec<BlobTask<T>>,
}

impl<T> WhenAny<T>
where
    T: Clone + Send + 'static,
{
    /// Creates a new WhenAny combinator from a vector of futures
    pub fn from_futures(futures: Vec<Pin<Box<dyn Future<Output = T> + Send>>>) -> Self {
        return Self {
            tasks: futures.into_iter().map(BlobTask::from_future).collect(),
        };
    }

    pub fn from_blob_tasks(tasks: Vec<BlobTask<T>>) -> Self {
        return Self { tasks };
    }
}

impl<T> Future for WhenAny<T>
where
    T: Clone + Send + 'static,
{
    type Output = WhenAnyResult<T>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: no self-referential data, safe to get mutable reference
        let this = unsafe { self.get_unchecked_mut() };

        if this.tasks.is_empty() {
            return Poll::Pending;
        }

        for (index, task) in this.tasks.iter_mut().enumerate() {
            let pinned_task = unsafe { Pin::new_unchecked(task) };
            match pinned_task.poll(ctx) {
                Poll::Ready(result) => {
                    return Poll::Ready(WhenAnyResult { result, index });
                }
                Poll::Pending => continue,
            }
        }

        return Poll::Pending;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_task::ToBlobTaskExt;
    use crate::completed_task::CompletedTask;
    use crate::task_utils;
    use std::time::Duration;

    #[tokio::test]
    async fn when_all_with_completed_tasks() {
        let results = crate::when_all!(CompletedTask::new(1), CompletedTask::new(2), CompletedTask::new(3)).await;

        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn when_all_empty_vec() {
        let futures: Vec<BlobTask<i32>> = vec![];
        let results = WhenAll::from_blob_tasks(futures).await;

        assert_eq!(results, Vec::<i32>::new());
    }

    #[tokio::test]
    async fn when_all_single_future() {
        let results = crate::when_all!(CompletedTask::new(42)).await;
        assert_eq!(results, vec![42]);
    }

    #[tokio::test]
    async fn when_all_with_delays() {
        let future1 = async {
            task_utils::wait_for_millis(50).await;
            return 1;
        };
        let future2 = async {
            task_utils::wait_for_millis(100).await;
            return 2;
        };
        let future3 = async {
            task_utils::wait_for_millis(25).await;
            return 3;
        };
        let start = std::time::Instant::now();
        let results = crate::when_all!(future1, future2, future3).await;
        let elapsed = start.elapsed();

        assert_eq!(results, vec![1, 2, 3]);
        // Should wait for the longest one (~100ms)
        assert!(elapsed >= Duration::from_millis(95));
        assert!(elapsed < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn when_all_with_strings() {
        let results = crate::when_all!(
            CompletedTask::new(String::from("hello")),
            CompletedTask::new(String::from("world"))
        )
        .await;

        assert_eq!(results, vec![String::from("hello"), String::from("world")]);
    }

    #[tokio::test]
    async fn when_all_preserves_order() {
        let futures = vec![
            Box::pin(async {
                task_utils::wait_for_millis(100).await;
                return 1;
            }) as Pin<Box<dyn Future<Output = i32> + Send>>,
            Box::pin(async {
                task_utils::wait_for_millis(50).await;
                return 2;
            }),
            Box::pin(async {
                task_utils::wait_for_millis(75).await;
                return 3;
            }),
        ];

        let results = WhenAll::from_futures(futures).await;

        // Despite different completion times, order is preserved
        assert_eq!(results, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn when_any_with_completed_tasks() {
        let result = crate::when_any!(CompletedTask::new(1), CompletedTask::new(2), CompletedTask::new(3)).await;

        // First one should complete (but any is valid)
        assert_eq!(result.index, 0);
        assert_eq!(result.result, 1);
    }

    #[tokio::test]
    async fn when_any_single_future() {
        let result = crate::when_any!(CompletedTask::new(42)).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 42);
    }

    #[tokio::test]
    async fn when_any_returns_fastest() {
        let future1 = async {
            crate::wait_for_millis(100).await;
            return 1;
        };
        let future2 = async {
            crate::wait_for_millis(25).await;
            return 2;
        };
        let future3 = async {
            crate::wait_for_millis(50).await;
            return 3;
        };

        let start = std::time::Instant::now();
        let result = crate::when_any!(future1, future2, future3).await;
        let elapsed = start.elapsed();

        // Should return the fastest (index 1, value 2)
        assert_eq!(result.index, 1);
        assert_eq!(result.result, 2);
        // Should complete in ~25ms, not 100ms
        assert!(elapsed < Duration::from_millis(75));
    }

    #[tokio::test]
    async fn when_any_with_strings() {
        let result = crate::when_any!(
            CompletedTask::new(String::from("first")),
            CompletedTask::new(String::from("second"))
        )
        .await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, String::from("first"));
    }

    #[tokio::test]
    async fn when_all_struct_constructor() {
        let results = crate::when_all!(CompletedTask::new(10), CompletedTask::new(20)).await;
        assert_eq!(results, vec![10, 20]);
    }

    #[tokio::test]
    async fn when_any_struct_constructor() {
        let result = crate::when_any!(CompletedTask::new(99)).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 99);
    }

    #[tokio::test]
    async fn when_all_with_unit_type() {
        let results = crate::when_all!(CompletedTask::new(()), CompletedTask::new(()), CompletedTask::new(())).await;
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
        #[derive(Debug, PartialEq, Clone)]
        struct Data {
            id: i32,
            name: String,
        }

        let results = crate::when_all!(
            CompletedTask::new(Data {
                id: 1,
                name: String::from("first"),
            }),
            CompletedTask::new(Data {
                id: 2,
                name: String::from("second"),
            })
        )
        .await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[1].name, "second");
    }

    #[tokio::test]
    async fn when_any_with_different_completion_times() {
        let future1 = async {
            task_utils::wait_for_millis(200).await;
            return "slow";
        };
        let future2 = async {
            task_utils::wait_for_millis(50).await;
            return "fast";
        };
        let future3 = async {
            task_utils::wait_for_millis(300).await;
            return "slowest";
        };

        let result = crate::when_any!(future1, future2, future3).await;

        assert_eq!(result.result, "fast");
        assert_eq!(result.index, 1);
    }

    #[tokio::test]
    async fn when_all_large_number_of_futures() {
        let results = WhenAll::from_blob_tasks((0..100).map(|i| CompletedTask::new(i).to_blob_task()).collect()).await;

        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 0);
        assert_eq!(results[99], 99);
    }

    #[tokio::test]
    async fn when_any_large_number_of_futures() {
        let result = WhenAny::from_blob_tasks((0..100).map(|i| CompletedTask::new(i).to_blob_task()).collect()).await;

        assert_eq!(result.index, 0);
        assert_eq!(result.result, 0);
    }

    #[tokio::test]
    async fn when_all_mixed_delay_times() {
        let task1 = async {
            task_utils::wait_for_millis(30).await;
            return 1;
        };
        let task2 = async {
            task_utils::wait_for_millis(60).await;
            return 2;
        };
        let task3 = async {
            task_utils::wait_for_millis(45).await;
            return 3;
        };
        let task4 = async {
            task_utils::wait_for_millis(15).await;
            return 4;
        };

        let results = crate::when_all!(task1, task2, task3, task4).await;

        // Order preserved despite different times
        assert_eq!(results, vec![1, 2, 3, 4]);
    }
}
