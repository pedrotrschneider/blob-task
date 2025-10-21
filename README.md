# BlobTask: High-Performance Async Utilities

A UniTask-inspired async/await library for the Blob game engine, built on top of Tokio. Designed for maximum efficiency, minimal allocations, and ergonomic async patterns.

## Philosophy

This library follows UniTask's core principles:
- **Zero-allocation** wherever possible
- **Minimal dependencies** (only Tokio required)
- **Explicit control** over async operations
- **Type-safe** and memory-safe by design
- **Composable** primitives for complex async workflows
- **UniTask-like wrapper types** instead of raw futures

## Core Concepts

### BlobTask - The UniTask Equivalent

`BlobTask<T>` is our equivalent to UniTask's `UniTask<T>`. It's a clonable, awaitable wrapper around futures that:
- Runs the inner future **exactly once** even when cloned
- Can be awaited multiple times from different clones
- All clones receive the same result
- Automatically spawns on the Tokio runtime when first polled

**Why BlobTask?**
- Share the same async operation across multiple awaits
- Clone and pass around without re-executing
- Perfect for caching expensive async operations
- Similar ergonomics to C#'s `Task<T>` or UniTask's `UniTask<T>`

#### Usage Example

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
    // Both get the same result, but operation ran only once
    
    // Or use the extension trait
    let task = async { 42 }.to_blob_task();
    let result = task.await;
}
```

### Block and Forget - Synchronous Execution

Two traits for executing futures outside async contexts:

**Block** - Synchronously wait for a future to complete
**Forget** - Fire-and-forget execution (spawn and don't wait)

#### Usage Examples

```rust
use blob_task::{Block, Forget};

// Block - wait synchronously (NOT inside async fn!)
fn main() {
    let result = (async {
        fetch_data().await
    }).block();
    
    println!("Result: {}", result);
}

// Forget - spawn and don't wait
fn trigger_background_work() {
    (async {
        cleanup_database().await;
    }).forget();
    
    // Returns immediately, work continues in background
}
```

## Features Implemented

### ✅ CompletedTask - Immediately Resolved Futures

A zero-allocation future that completes immediately with a value. Perfect for APIs that need to return futures but have the result available synchronously.

#### Implementation Details
- **Size**: `sizeof(Option<T>)` - no overhead beyond the value itself
- **Allocation**: Zero heap allocations
- **Performance**: Completes in a single poll
- **Safety**: Panics if polled after completion (helps catch bugs)
- **Constraint**: Requires `T: Clone + Send + 'static` for consistency

#### Usage Example

```rust
use blob_task::{CompletedTask, task_utils};

#[tokio::main]
async fn main() {
    // Direct construction
    let task = CompletedTask::new(42);
    let result = task.await;
    assert_eq!(result, 42);

    // Using helper function
    let task = blob_task::completed_task("hello");
    let result = task.await;
    assert_eq!(result, "hello");
}
```

#### When to Use
- Returning futures from functions that have immediate results
- Implementing async traits where some branches are synchronous
- Testing async code with known values
- Cache hits in async functions

```rust
async fn get_cached_or_fetch(key: &str) -> impl Future<Output = String> {
    if let Some(cached) = check_cache(key) {
        // Return immediately without async overhead
        blob_task::completed_task(cached)
    } else {
        // Actually perform async work
        fetch_from_network(key).to_blob_task()
    }
}
```

---

### ✅ TaskCompletionSource - Manual Future Control

A powerful primitive that allows you to create a future and complete it manually from outside the async context. Similar to C#'s `TaskCompletionSource` or JavaScript's Promise constructor.

#### Implementation Details
- **Thread-safe**: Uses `Arc<Mutex<>>` for shared state
- **Cloneable**: Multiple handles can complete the same task
- **Waker system**: Efficiently notifies waiting tasks on completion
- **Multiple consumers**: Multiple tasks can await the same source
- **Idempotent**: Completing twice returns `false` but doesn't panic
- **Requires Clone**: The result type must implement `Clone` for multiple consumers

#### Usage Example

```rust
use blob_task::TaskCompletionSource;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let tcs = blob_task::task_completion_source();
    let task = tcs.task();

    // Complete from another thread
    let tcs_clone = tcs.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        tcs_clone.complete(42);
    });

    // Await the result
    let result = task.await;
    println!("Result: {}", result);
}
```

#### Advanced Pattern: Bridging Callback-Based APIs

```rust
use blob_task::TaskCompletionSource;

async fn async_wrapper() -> String {
    let tcs = TaskCompletionSource::new();
    let tcs_clone = tcs.clone();
    
    legacy_api_with_callback(move |data| {
        tcs_clone.complete(data);
    });
    
    tcs.task().await
}
```

---

### ✅ Delay and Yield - Time-Based and Cooperative Scheduling

Utilities for delaying execution and yielding control to the executor, built on top of Tokio's time primitives.

#### Implementation Details
- **Delay**: Thin wrapper around `tokio::time::sleep`
- **Yield**: Single cooperative yield to the executor
- **YieldMany**: Yields N times before completing
- **Zero overhead**: Direct integration with Tokio's scheduler

#### Usage Examples

**Delay**
```rust
use blob_task::{task_utils, Delay};
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Using helper functions (recommended)
    blob_task::wait_for_millis(500).await;
    blob_task::wait_for_seconds(2).await;
    blob_task::delay(Duration::from_secs(1)).await;
    
    // Direct construction
    Delay::new(Duration::from_secs(1)).await;
}
```

**Yield**
```rust
use blob_task::task_utils;

#[tokio::main]
async fn main() {
    // Yield control once to allow other tasks to run
    blob_task::yield_once().await;
    
    // Useful in loops to prevent blocking the executor
    for i in 0..1000 {
        process_item(i);
        
        // Periodically yield to other tasks
        if i % 100 == 0 {
            blob_task::yield_once().await;
        }
    }
    
    // Yield multiple times
    blob_task::yield_many(5).await;
}
```

---

### ✅ CancellationToken - Cooperative Cancellation

A thread-safe cancellation token system for signaling and observing cancellation across async operations.

#### Implementation Details
- **Thread-safe**: Uses `Arc<Mutex<>>` for shared state
- **Hierarchical**: Support for parent-child token relationships
- **Waker notification**: Efficiently wakes waiting futures
- **Cloneable**: Share across tasks and threads

#### Usage Examples

**Basic Cancellation**
```rust
use blob_task::CancellationToken;

#[tokio::main]
async fn main() {
    let token = blob_task::cancellation_token();
    let token_clone = token.clone();
    
    tokio::spawn(async move {
        // Wait for cancellation
        token_clone.cancelled_future().await;
        println!("Task was cancelled!");
    });
    
    // Later, cancel the operation
    token.cancel();
}
```

**Timeout Pattern with tokio::select!**
```rust
use blob_task::CancellationToken;

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
    let token_clone = token.clone();
    
    let result = tokio::select! {
        _ = token_clone.cancelled_future() => {
            Err("Cancelled")
        }
        result = do_work() => {
            Ok(result)
        }
    };
}
```

**Parent-Child Tokens**
```rust
let parent = blob_task::cancellation_token();
let child = parent.new_child();

// Cancelling parent also cancels all children
parent.cancel();

assert!(child.is_cancelled());
```

---

### ✅ WhenAll and WhenAny - Future Combinators

Utilities for combining multiple futures, similar to `Promise.all()` and `Promise.race()` in JavaScript. Uses convenient macros and BlobTask under the hood.

#### Implementation Details
- **WhenAll**: Waits for all futures, preserves order
- **WhenAny**: Returns first completed future with its index
- **BlobTask-based**: Ensures each future runs only once
- **Macro syntax**: Ergonomic syntax similar to UniTask

#### Usage Examples

**WhenAll - Wait for Multiple Futures**
```rust
use blob_task::{when_all, CompletedTask};

#[tokio::main]
async fn main() {
    // Using the macro (recommended)
    let results = when_all!(
        CompletedTask::new(1),
        CompletedTask::new(2),
        CompletedTask::new(3),
    ).await;
    
    assert_eq!(results, vec![1, 2, 3]);
}
```

**WhenAll with Async Blocks**
```rust
use blob_task::{when_all, task_utils};

#[tokio::main]
async fn main() {
    let results = when_all!(
        async {
            blob_task::wait_for_millis(100).await;
            1
        },
        async {
            blob_task::wait_for_millis(50).await;
            2
        },
        async {
            blob_task::wait_for_millis(75).await;
            3
        },
    ).await;
    
    // Results are in original order, not completion order
    assert_eq!(results, vec![1, 2, 3]);
}
```

**WhenAny - Race Multiple Futures**
```rust
use blob_task::{when_any, task_utils};

#[tokio::main]
async fn main() {
    let result = when_any!(
        async {
            blob_task::wait_for_millis(200).await;
            "slow"
        },
        async {
            blob_task::wait_for_millis(50).await;
            "fast"
        },
        async {
            blob_task::wait_for_millis(300).await;
            "slowest"
        },
    ).await;
    
    assert_eq!(result.result, "fast");
    assert_eq!(result.index, 1);
}
```

**Practical Timeout Pattern**
```rust
use blob_task::{when_any, task_utils};

async fn fetch_with_timeout(url: &str) -> Result<String, &'static str> {
    let result = when_any!(
        async { 
            Ok(fetch_url(url).await)
        },
        async {
            blob_task::wait_for_seconds(5).await;
            Err("timeout")
        },
    ).await;
    
    result.result
}
```

---

### ✅ ObjectPool - Reusable Object Allocation

Thread-safe object pooling for reducing allocations in high-performance scenarios.

#### Implementation Details
- **Thread-safe**: Uses `Arc<Mutex<>>` for shared state
- **RAII-based**: Objects automatically return to pool via Drop
- **Capacity limits**: Prevent unbounded growth
- **Factory pattern**: Custom object creation
- **Optional reset**: Clean objects before reuse

#### Usage Examples

**Basic Object Pool**
```rust
use blob_task::ObjectPool;

#[tokio::main]
async fn main() {
    // Create pool with factory function
    let pool = ObjectPool::new(|| Vec::<u8>::new());
    
    {
        let mut buffer = pool.rent();
        buffer.extend_from_slice(&[1, 2, 3, 4]);
        // buffer automatically returns to pool when dropped
    }
    
    // Object is now available for reuse
    assert_eq!(pool.available(), 1);
}
```

**Pool with Reset Function**
```rust
use blob_task::ObjectPoolWithReset;

let pool = ObjectPoolWithReset::new(
    || Vec::<u8>::new(),
    |v| v.clear(), // Reset function cleans before returning
);

{
    let mut buffer = pool.rent();
    buffer.extend_from_slice(&[1, 2, 3]);
}

// Next rent gets a clean, empty vector
let buffer = pool.rent();
assert_eq!(buffer.len(), 0);
```

---

## Helper Functions (task_utils)

The library provides convenient helper functions in the `task_utils` module for common operations:

```rust
use blob_task::task_utils;

// CompletedTask
let task = blob_task::completed_task(42);

// Delays
blob_task::delay(Duration::from_secs(1)).await;
blob_task::wait_for_millis(100).await;
blob_task::wait_for_seconds(2).await;

// Yields
blob_task::yield_once().await;
blob_task::yield_many(5).await;

// CancellationToken
let token = blob_task::cancellation_token();

// TaskCompletionSource
let tcs = blob_task::task_completion_source::<i32>();
```

---

## Project Structure

```
src/
├── lib.rs                      # Public API exports
├── blob_task.rs                # BlobTask wrapper (UniTask equivalent)
├── block_forget.rs             # Block and Forget traits
├── completed_task.rs           # CompletedTask implementation
├── task_completion_source.rs  # TaskCompletionSource implementation
├── delay_yield.rs              # Delay, Yield, YieldMany
├── cancellation_token.rs       # CancellationToken implementation
├── when_combinators.rs         # WhenAll, WhenAny with macros
├── object_pool.rs              # ObjectPool implementations
└── task_utils.rs               # Helper functions
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt", "macros", "sync", "time"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros", "time", "rt-multi-thread"] }
```

## Testing

Comprehensive test coverage for all features (120+ tests):

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific module tests
cargo test completed_task
cargo test task_completion_source
cargo test delay_yield
cargo test cancellation_token
cargo test when_combinators
cargo test object_pool
cargo test blob_task
cargo test block_forget
```

## Complete Feature List

✅ **BlobTask** - UniTask-like wrapper around futures

✅ **Block/Forget** - Synchronous execution and fire-and-forget

✅ **CompletedTask** - Immediately resolved futures

✅ **TaskCompletionSource** - Manual future completion

✅ **Delay/Yield utilities** - Time-based and cooperative yielding

✅ **CancellationToken** - Cooperative cancellation support

✅ **WhenAll/WhenAny** - Future combinators with macros

✅ **ObjectPool** - Allocation pooling for performance

✅ **Helper functions** - Convenient wrappers in task_utils

## Design Principles

### 1. UniTask-Inspired API
Provides familiar patterns for developers coming from Unity/C# while maintaining Rust's safety guarantees.

### 2. Zero-Cost Abstractions
Every abstraction is designed to compile down to efficient machine code with no runtime overhead.

### 3. Memory Safety
Leverages Rust's ownership system to prevent data races and memory leaks without runtime checks.

### 4. Composability
Small, focused primitives that can be combined to build complex async workflows.

### 5. Explicit Over Implicit
Clear APIs that make the cost and behavior of operations obvious.

### 6. Tokio Integration
Built on top of Tokio, not reinventing the wheel. We add ergonomic utilities, not replace the runtime.

## Performance Characteristics

| Feature | Allocation | Poll Overhead | Thread-Safe | Runs Once |
|---------|-----------|---------------|-------------|-----------|
| BlobTask | Arc+Mutex | Spawn overhead | Yes | Yes |
| CompletedTask | Zero | Single poll | N/A | N/A |
| TaskCompletionSource | Arc+Mutex | Minimal | Yes | N/A |
| Delay | Tokio heap | Tokio overhead | Yes | N/A |
| Yield | Zero | Single poll | Yes | N/A |
| CancellationToken | Arc+Mutex | Minimal | Yes | N/A |
| WhenAll/WhenAny | Arc+Mutex (via BlobTask) | Per-future poll | Yes | Yes |
| ObjectPool | Arc+Mutex+Vec | Lock contention | Yes | N/A |

## Examples

### Complete Example: Async Web Request with Timeout and Cancellation

```rust
use blob_task::{when_any, CancellationToken, Block};

async fn fetch_with_timeout_and_cancellation(
    url: &str,
    token: CancellationToken,
) -> Result<String, &'static str> {
    let result = blob_task::when_any!(
        async {
            fetch_url(url).await.map_err(|_| "fetch failed")
        },
        async {
            blob_task::wait_for_seconds(5).await;
            Err("timeout")
        },
        async {
            token.cancelled_future().await;
            Err("cancelled")
        },
    ).await;
    
    result.result
}

// Can be called from sync context with .block()
fn main() {
    let token = blob_task::cancellation_token();
    
    let result = fetch_with_timeout_and_cancellation(
        "https://example.com",
        token
    ).block();
    
    println!("{:?}", result);
}
```

### Complete Example: Parallel Processing with Pool

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
    
    WhenAll::from_blob_tasks(tasks).await
}
```

## Contributing

This is a learning project focused on building high-performance async utilities. Contributions and suggestions welcome!

## License

[Your chosen license]

---

## FAQ

### Q: How is this different from Tokio's built-in utilities?

**A**: This library provides higher-level ergonomic patterns inspired by UniTask, while Tokio focuses on the runtime and low-level primitives. We build on top of Tokio to provide common patterns like manual task completion, object pooling, BlobTask wrappers, and specialized future types.

### Q: What is BlobTask and why should I use it?

**A**: `BlobTask<T>` is our equivalent to UniTask's `UniTask<T>`. It wraps a future and ensures it runs exactly once, even when cloned. All clones receive the same result. Use it when you need to share an async operation across multiple awaits without re-execution - perfect for caching, sharing results, or implementing async lazy initialization.

### Q: When should I use Block vs regular .await?

**A**: Use `Block` only in **synchronous contexts** (like `main()` or callback handlers) where you need to wait for an async operation. Never use it inside async functions - use `.await` instead. `Block` creates or uses a Tokio runtime to execute the future synchronously.

### Q: What's the difference between Forget and tokio::spawn?

**A**: `Forget` is a convenience trait that works in both sync and async contexts. If a runtime exists, it uses `tokio::spawn`. If not, it creates a temporary runtime on a new thread. It's fire-and-forget - you don't get a JoinHandle back.

### Q: Why do the when_all! and when_any! use macros?

**A**: The macros provide a clean syntax and automatically convert futures to `BlobTask`, ensuring each future runs only once. They're more ergonomic than manually constructing vectors and calling methods.

### Q: Is TaskCompletionSource thread-safe?

**A**: Yes! You can clone it and complete it from any thread. The future will be properly notified.

### Q: Why not use channels instead of TaskCompletionSource?

**A**: TaskCompletionSource is designed for **single-value, one-time completion**. Channels are for streams of values. TaskCompletionSource is more efficient for the one-shot case and has better ergonomics for bridging callback APIs.

### Q: When should I use ObjectPool?

**A**: Use ObjectPool when you're frequently allocating and deallocating the same type of object (like buffers or connections) in performance-critical code. Profile first - premature optimization is the root of all evil!

### Q: Why does everything require T: Clone + Send + 'static?

**A**: These bounds ensure type safety and thread safety across the library. `Clone` allows BlobTask to share results, `Send` ensures thread safety, and `'static` prevents lifetime issues with spawned tasks.