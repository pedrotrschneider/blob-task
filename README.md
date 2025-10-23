# BlobTask

A high-performance async utilities library for Rust, inspired by Unity's UniTask. Built on Tokio with a focus on ergonomics, zero-cost abstractions, and practical async patterns.

## Features

- **Zero-allocation** futures where possible
- **Composable** async primitives for complex workflows
- **UniTask-inspired** API for familiar patterns
- **Type-safe** cancellation and completion
- **Minimal dependencies** (only Tokio and pin-project-lite)

## Quick Start

```toml
[dependencies]
blob-task = "0.1"
tokio = { version = "1", features = ["rt", "macros", "time"] }
```

## Core Types

### BlobTask - Shared Task Execution

A clonable, awaitable wrapper that runs its inner future exactly once, even when cloned multiple times.

```rust
use blob_task::{BlobTask, ToBlobTaskExt};

#[tokio::main]
async fn main() {
    // Create from a future
    let task = BlobTask::from_future(async {
        expensive_operation().await
    });
    
    // Clone and await multiple times - runs only once!
    let task1 = task.clone();
    let task2 = task.clone();
    
    let (r1, r2) = tokio::join!(task1, task2);
    // Both receive the same result
    
    // Or use the extension trait
    let task = async { 42 }.to_blob_task();
}
```

### BlobLazyTask - Lazy Initialization

Defers creation of the async task until first awaited. Perfect for lazy statics and expensive initialization.

```rust
use blob_task::BlobLazyTask;

// Factory runs only on first await
let lazy = BlobLazyTask::new(|| async {
    expensive_initialization().await
});

// First await runs the factory
let result = lazy.clone().await;

// Subsequent awaits reuse the same task
let same_result = lazy.await;
```

### CompletedTask - Immediate Results

Zero-allocation future that completes immediately with a value.

```rust
use blob_task::CompletedTask;

#[tokio::main]
async fn main() {
    let task = CompletedTask::new(42);
    let result = task.await; // Ready immediately
    
    // Helper function
    let task = blob_task::completed_task("hello");
}
```

### TaskCompletionSource - Manual Completion

Create a future and complete it manually from any context.

```rust
use blob_task::TaskCompletionSource;

#[tokio::main]
async fn main() {
    let tcs = TaskCompletionSource::new();
    let task = tcs.task();
    
    // Complete from another thread
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        tcs.complete(42);
    });
    
    let result = task.await;
}
```

## Synchronous Execution

### Block - Wait Synchronously

Execute futures in non-async contexts.

```rust
use blob_task::Block;

fn main() {
    let result = (async {
        fetch_data().await
    }).block();
    
    println!("Result: {}", result);
}
```

### Forget - Fire and Forget

Spawn tasks without waiting for completion.

```rust
use blob_task::Forget;

fn trigger_background_work() {
    (async {
        cleanup_database().await;
    }).forget();
    // Returns immediately
}
```

## Time and Yielding

### Delays

```rust
use blob_task::{delay, wait_for_millis, wait_for_seconds};

#[tokio::main]
async fn main() {
    // Various ways to delay
    delay(Duration::from_secs(1)).await;
    wait_for_millis(500).await;
    wait_for_seconds(2).await;
}
```

### Yielding

```rust
use blob_task::{yield_once, yield_many};

#[tokio::main]
async fn main() {
    // Yield control to other tasks
    yield_once().await;
    
    // Yield multiple times
    yield_many(5).await;
    
    // Useful in loops
    for i in 0..1000 {
        process_item(i);
        if i % 100 == 0 {
            yield_once().await;
        }
    }
}
```

## Cancellation

### CancellationToken

Thread-safe cancellation with parent-child relationships.

```rust
use blob_task::CancellationToken;

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
    let token_clone = token.clone();
    
    tokio::spawn(async move {
        token_clone.cancelled_future().await;
        println!("Cancelled!");
    });
    
    // Cancel from anywhere
    token.cancel();
    
    // Parent-child relationships
    let parent = CancellationToken::new();
    let child = parent.new_child();
    parent.cancel(); // Also cancels child
}
```

### Timeout

Add timeouts to any future.

```rust
use blob_task::TimeoutExt;

#[tokio::main]
async fn main() {
    let result = async {
        slow_operation().await
    }
    .timeout_seconds(5)
    .await;
    
    match result {
        Ok(value) => println!("Completed: {}", value),
        Err(_) => println!("Timed out"),
    }
}
```

## Future Combinators

### WhenAll - Wait for All

```rust
use blob_task::when_all;

#[tokio::main]
async fn main() {
    let results = when_all!(
        async { fetch_user(1).await },
        async { fetch_user(2).await },
        async { fetch_user(3).await },
    ).await;
    
    // Results are in order
    println!("{:?}", results);
}
```

### WhenAny - Race Multiple Futures

```rust
use blob_task::when_any;

#[tokio::main]
async fn main() {
    let result = when_any!(
        async { source_a().await },
        async { source_b().await },
        async { source_c().await },
    ).await;
    
    println!("Winner: {} with result: {}", result.index, result.result);
}
```

### ContinueWith - Chain Futures

```rust
use blob_task::ContinueWithExt;

#[tokio::main]
async fn main() {
    let result = async { fetch_data().await }
        .continue_with(|data| async move {
            process_data(data).await
        })
        .continue_with(|processed| async move {
            save_data(processed).await
        })
        .await;
}
```

## Conditional Waiting

### WaitUntil / WaitWhile

Wait for predicates to become true or false.

```rust
use blob_task::{wait_until, wait_while};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::main]
async fn main() {
    let ready = Arc::new(AtomicBool::new(false));
    let ready_clone = ready.clone();
    
    // Wait until ready becomes true
    wait_until(move || ready_clone.load(Ordering::SeqCst)).await;
    
    let busy = Arc::new(AtomicBool::new(true));
    let busy_clone = busy.clone();
    
    // Wait while busy is true
    wait_while(move || busy_clone.load(Ordering::SeqCst)).await;
}
```

### WaitForValueChanged / WaitForValueEquals

Monitor value changes.

```rust
use blob_task::{wait_for_value_changed, wait_for_value_equals};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicUsize::new(0));
    let initial = counter.load(Ordering::SeqCst);
    
    let counter_clone = counter.clone();
    
    // Wait for value to change from initial
    let new_value = wait_for_value_changed(
        move || counter_clone.load(Ordering::SeqCst),
        initial
    ).await;
    
    let counter_clone = counter.clone();
    
    // Wait for specific value
    wait_for_value_equals(
        move || counter_clone.load(Ordering::SeqCst),
        10
    ).await;
}
```

## Resource Management

### ObjectPool

Thread-safe object pooling for reducing allocations.

```rust
use blob_task::ObjectPool;

#[tokio::main]
async fn main() {
    // Basic pool
    let pool = ObjectPool::new(|| Vec::<u8>::new());
    
    {
        let mut buffer = pool.rent();
        buffer.extend_from_slice(&[1, 2, 3, 4]);
        // Automatically returned to pool on drop
    }
    
    // Pool with reset function
    let pool = ObjectPoolWithReset::new(
        || Vec::<u8>::new(),
        |v| v.clear()
    );
    
    let mut buffer = pool.rent();
    buffer.push(42);
    // Cleared and returned to pool on drop
}
```

## Practical Examples

### Fetch with Timeout and Cancellation

```rust
use blob_task::{when_any, CancellationToken, TimeoutExt};

async fn fetch_with_controls(url: &str, token: CancellationToken) 
    -> Result<String, &'static str> 
{
    let result = when_any!(
        async { 
            fetch_url(url).await.map_err(|_| "fetch failed")
        },
        async {
            token.cancelled_future().await;
            Err("cancelled")
        }
    )
    .timeout_seconds(5)
    .await;
    
    match result {
        Ok(when_any_result) => when_any_result.result,
        Err(_) => Err("timeout"),
    }
}
```

### Bridging Callback APIs

```rust
use blob_task::TaskCompletionSource;

async fn callback_to_async() -> String {
    let tcs = TaskCompletionSource::new();
    let tcs_clone = tcs.clone();
    
    legacy_api_with_callback(move |data| {
        tcs_clone.complete(data);
    });
    
    tcs.task().await
}
```

### Parallel Processing with Pooling

```rust
use blob_task::{ObjectPool, when_all, BlobTask};

async fn process_batch(items: Vec<Item>) -> Vec<Result> {
    let pool = ObjectPool::new(|| Vec::with_capacity(100));
    
    let tasks: Vec<BlobTask<Result>> = items
        .into_iter()
        .map(|item| {
            let pool = pool.clone();
            BlobTask::from_future(async move {
                let mut buffer = pool.rent();
                process_item(item, &mut buffer).await
            })
        })
        .collect();
    
    when_all!(tasks).await
}
```

### Lazy Static Initialization

```rust
use blob_task::BlobLazyTask;
use once_cell::sync::Lazy;

static CONFIG: Lazy<BlobLazyTask<Config, _, _>> = Lazy::new(|| {
    BlobLazyTask::new(|| async {
        load_config_from_file().await
    })
});

async fn get_config() -> Config {
    CONFIG.clone().await
}
```

## Helper Functions

All major types have convenient helper functions in the root module:

```rust
use blob_task::*;

// CompletedTask
let task = completed_task(42);

// TaskCompletionSource
let tcs = task_completion_source::<i32>();

// CancellationToken
let token = cancellation_token();

// Delays
delay(Duration::from_secs(1)).await;
wait_for_millis(100).await;
wait_for_seconds(2).await;

// Yields
yield_once().await;
yield_many(5).await;

// Waiting
wait_until(|| condition()).await;
wait_while(|| condition()).await;
wait_for_value_changed(|| get_value(), initial).await;
wait_for_value_equals(|| get_value(), target).await;
```

## Testing

Run the comprehensive test suite:

```bash
cargo test
cargo test -- --nocapture
cargo test <module_name>
```

## Performance Characteristics

| Feature | Allocation | Thread-Safe | Overhead |
|---------|-----------|-------------|----------|
| BlobTask | Arc+Mutex | Yes | Spawn cost |
| BlobLazyTask | Arc+Mutex | Yes | First poll only |
| CompletedTask | Zero | N/A | Single poll |
| TaskCompletionSource | Arc+Mutex | Yes | Minimal |
| Delay | Tokio heap | Yes | Tokio timer |
| Yield | Zero | Yes | Single poll |
| CancellationToken | Arc+Mutex | Yes | Minimal |
| ObjectPool | Arc+Mutex+Vec | Yes | Lock contention |

## License

MIT License - see LICENSE file for details
