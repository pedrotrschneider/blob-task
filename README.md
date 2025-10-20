# Blob Task: High-Performance Async Utilities for the Blob Game Engine

A UniTask-inspired async/await library for Rust, built on top of Tokio. Designed for maximum efficiency, minimal allocations, and ergonomic async patterns.

## Philosophy

This library follows UniTask's core principles:
- **Zero-allocation** wherever possible
- **Minimal dependencies** (only Tokio required)
- **Explicit control** over async operations
- **Type-safe** and memory-safe by design
- **Composable** primitives for complex async workflows

## Features Implemented

### ✅ CompletedTask - Immediately Resolved Futures

A zero-allocation future that completes immediately with a value. Perfect for APIs that need to return futures but have the result available synchronously.

#### Implementation Details
- **Size**: `sizeof(Option<T>)` - no overhead beyond the value itself
- **Allocation**: Zero heap allocations
- **Performance**: Completes in a single poll
- **Safety**: Panics if polled after completion (helps catch bugs)

#### Usage Example

```rust
use your_async_lib::CompletedTask;

#[tokio::main]
async fn main() {
    // Direct construction
    let task = CompletedTask::new(42);
    let result = task.await;
    assert_eq!(result, 42);

    // Using from_result method
    let task = CompletedTask::from_result("hello");
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
        CompletedTask::from_result(cached)
    } else {
        // Actually perform async work
        fetch_from_network(key)
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

#### Architecture
```
TaskCompletionSource (clonable handle)
         ↓
    Arc<Mutex<SharedState>>
         ↓
    TaskCompletionFuture (awaitable)
```

#### Usage Example

```rust
use your_async_lib::TaskCompletionSource;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let tcs = TaskCompletionSource::new();
    let task = tcs.task();

    // Complete from another thread
    let tcs_clone = tcs.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        tcs_clone.set_result(42);
    });

    // Await the result
    let result = task.await;
    println!("Result: {}", result);
}
```

#### Advanced Pattern: Bridging Callback-Based APIs

```rust
use your_async_lib::TaskCompletionSource;

// Convert a callback-based API to async/await
fn legacy_api_with_callback<F>(callback: F) 
where 
    F: FnOnce(String) + Send + 'static 
{
    // Simulates an old callback-based API
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        callback(String::from("data"));
    });
}

async fn async_wrapper() -> String {
    let tcs = TaskCompletionSource::new();
    let task = tcs.task();
    
    legacy_api_with_callback(move |data| {
        tcs.set_result(data);
    });
    
    task.await
}
```

#### Multiple Awaits Pattern

```rust
let tcs = TaskCompletionSource::new();

// Multiple tasks can await the same source
let task1 = tcs.task();
let task2 = tcs.task();
let task3 = tcs.task();

// Complete once
tcs.set_result(100);

// All tasks receive the same value (via Clone)
let (r1, r2, r3) = tokio::join!(task1, task2, task3);
assert_eq!((r1, r2, r3), (100, 100, 100));
```

#### Checking Completion State

```rust
let tcs = TaskCompletionSource::new();

// Check if completed
assert!(!tcs.is_completed());

tcs.set_result(42);

assert!(tcs.is_completed());

// Try to get result without awaiting (requires Clone)
if let Some(value) = tcs.try_get_result() {
    println!("Already completed with: {}", value);
}
```

#### When to Use
- **Bridging APIs**: Converting callback-based code to async/await
- **Manual control**: When you need precise control over when a future completes
- **Synchronization**: Coordinating between sync and async code
- **Event notifications**: Implementing one-shot events
- **Testing**: Controlling async flow in tests

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
use your_async_lib::Delay;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Delay for a duration
    Delay::new(Duration::from_secs(1)).await;
    
    // Convenience methods
    Delay::millis(500).await;
    Delay::seconds(2).await;
}
```

**Yield**
```rust
use your_async_lib::Yield;

#[tokio::main]
async fn main() {
    // Yield control once to allow other tasks to run
    Yield::new().await;
    
    // Useful in loops to prevent blocking the executor
    for i in 0..1000 {
        // Do work
        process_item(i);
        
        // Periodically yield to other tasks
        if i % 100 == 0 {
            Yield::new().await;
        }
    }
}
```

**YieldMany**
```rust
use your_async_lib::YieldMany;

#[tokio::main]
async fn main() {
    // Yield control 5 times
    YieldMany::new(5).await;
    
    // Useful for distributing work across multiple polls
}
```

#### When to Use
- **Delay**: Timeouts, rate limiting, scheduled tasks
- **Yield**: Preventing task starvation in tight loops
- **YieldMany**: Distributing expensive computations across polls

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
use your_async_lib::CancellationToken;

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
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
use your_async_lib::{CancellationToken, Delay};

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
use your_async_lib::CancellationToken;

#[tokio::main]
async fn main() {
    let parent = CancellationToken::new();
    let child = parent.new_child();
    
    // Cancelling parent also cancels all children
    parent.cancel();
    
    assert!(child.is_cancelled());
}
```

**Checking State**
```rust
let token = CancellationToken::new();

if token.is_cancelled() {
    // Already cancelled
    return;
}

// Do work...
```

#### When to Use
- **Timeout operations**: Cancel work that takes too long
- **User cancellation**: Respond to "Cancel" button clicks
- **Graceful shutdown**: Coordinate shutdown of multiple tasks
- **Resource cleanup**: Signal dependent resources to clean up

---

### ✅ WhenAll and WhenAny - Future Combinators

Utilities for combining multiple futures, similar to `Promise.all()` and `Promise.race()` in JavaScript.

#### Implementation Details
- **WhenAll**: Waits for all futures, preserves order
- **WhenAny**: Returns first completed future with its index
- **Generic**: Works with any future type
- **Efficient**: Minimal allocations

#### Usage Examples

**WhenAll - Wait for Multiple Futures**
```rust
use your_async_lib::{when_all, CompletedTask};

#[tokio::main]
async fn main() {
    let futures = vec![
        CompletedTask::new(1),
        CompletedTask::new(2),
        CompletedTask::new(3),
    ];
    
    let results = when_all(futures).await;
    assert_eq!(results, vec![1, 2, 3]);
}
```

**WhenAll with Mixed Delays**
```rust
use your_async_lib::Delay;

#[tokio::main]
async fn main() {
    let futures = vec![
        Box::pin(async {
            Delay::millis(100).await;
            1
        }) as Pin<Box<dyn Future<Output = i32> + Send>>,
        Box::pin(async {
            Delay::millis(50).await;
            2
        }),
    ];
    
    let results = when_all(futures).await;
    // Results are in original order, not completion order
    assert_eq!(results, vec![1, 2]);
}
```

**WhenAny - Race Multiple Futures**
```rust
use your_async_lib::{when_any, Delay};

#[tokio::main]
async fn main() {
    let futures = vec![
        Box::pin(async {
            Delay::millis(100).await;
            "slow"
        }) as Pin<Box<dyn Future<Output = &str> + Send>>,
        Box::pin(async {
            Delay::millis(50).await;
            "fast"
        }),
    ];
    
    let result = when_any(futures).await;
    assert_eq!(result.result, "fast");
    assert_eq!(result.index, 1);
}
```

**Practical Timeout Pattern**
```rust
use your_async_lib::{when_any, Delay};

async fn fetch_with_timeout(url: &str) -> Result<String, &'static str> {
    let futures = vec![
        Box::pin(fetch_url(url)) as Pin<Box<dyn Future<Output = Result<String, &'static str>> + Send>>,
        Box::pin(async {
            Delay::seconds(5).await;
            Err("timeout")
        }),
    ];
    
    let result = when_any(futures).await;
    result.result
}
```

#### When to Use
- **WhenAll**: Parallel operations that all must complete (batch processing, parallel downloads)
- **WhenAny**: Racing operations (timeout, fastest mirror, fallback servers)

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
use your_async_lib::ObjectPool;

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

**Pool with Capacity Limit**
```rust
use your_async_lib::ObjectPool;

let pool = ObjectPool::with_capacity(|| Vec::<u8>::new(), 10);

// Pool will only keep up to 10 objects
// Extra objects are dropped instead of pooled
```

**Pool with Reset Function**
```rust
use your_async_lib::ObjectPoolWithReset;

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

**Taking Ownership**
```rust
let pool = ObjectPool::new(|| Vec::<u8>::new());

let mut obj = pool.rent();
obj.push(42);

// Take ownership, preventing return to pool
let owned = obj.take();
assert_eq!(pool.available(), 0);
```

**Concurrent Usage**
```rust
use your_async_lib::ObjectPool;

#[tokio::main]
async fn main() {
    let pool = ObjectPool::new(|| Vec::<u8>::new());
    
    let mut handles = vec![];
    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let mut buffer = pool_clone.rent();
            buffer.push(i);
            // Automatically returned
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Objects are now available for reuse
    println!("Available: {}", pool.available());
}
```

**Custom Types**
```rust
struct Connection {
    url: String,
    connected: bool,
}

let pool = ObjectPoolWithReset::new(
    || Connection {
        url: String::from("localhost"),
        connected: false,
    },
    |conn| {
        conn.connected = false; // Reset state
    },
);
```

#### Pool Management

```rust
let pool = ObjectPool::new(|| Vec::<u8>::new());

// Check available objects
println!("Available: {}", pool.available());

// Clear all pooled objects
pool.clear();

assert_eq!(pool.available(), 0);
```

#### When to Use
- **Buffer pooling**: Reuse byte buffers, string builders
- **Connection pooling**: Database connections, HTTP clients
- **Temporary objects**: Frequently created/destroyed objects in hot paths
- **Reducing GC pressure**: Minimize allocations in performance-critical code

---

## Project Structure

```
src/
├── lib.rs                      # Public API exports
├── completed_task.rs           # CompletedTask implementation
├── task_completion_source.rs  # TaskCompletionSource implementation
├── delay_yield.rs              # Delay, Yield, YieldMany
├── cancellation_token.rs       # CancellationToken implementation
├── when_combinators.rs         # WhenAll, WhenAny combinators
└── object_pool.rs              # ObjectPool implementations
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt", "macros", "sync", "time"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros", "time", "rt-multi-thread"] }
```

## Testing

Comprehensive test coverage for all features (100+ tests):

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
```

## Complete Feature List

✅ **CompletedTask** - Immediately resolved futures
✅ **TaskCompletionSource** - Manual future completion
✅ **Delay/Yield utilities** - Time-based and cooperative yielding
✅ **CancellationToken** - Cooperative cancellation support
✅ **WhenAll/WhenAny** - Future combinators
✅ **ObjectPool** - Allocation pooling for performance

## Design Principles

### 1. Zero-Cost Abstractions
Every abstraction is designed to compile down to efficient machine code with no runtime overhead.

### 2. Memory Safety
Leverages Rust's ownership system to prevent data races and memory leaks without runtime checks.

### 3. Composability
Small, focused primitives that can be combined to build complex async workflows.

### 4. Explicit Over Implicit
Clear APIs that make the cost and behavior of operations obvious.

### 5. Tokio Integration
Built on top of Tokio, not reinventing the wheel. We add ergonomic utilities, not replace the runtime.

## Performance Characteristics

| Feature | Allocation | Poll Overhead | Thread-Safe |
|---------|-----------|---------------|-------------|
| CompletedTask | Zero | Single poll | N/A (immediate) |
| TaskCompletionSource | Arc+Mutex | Minimal | Yes |
| Delay | Tokio heap | Tokio overhead | Yes |
| Yield | Zero | Single poll | Yes |
| CancellationToken | Arc+Mutex | Minimal | Yes |
| WhenAll/WhenAny | Vec+Box | Per-future poll | N/A |
| ObjectPool | Arc+Mutex+Vec | Lock contention | Yes |

## Contributing

This is a learning project focused on building high-performance async utilities. Contributions and suggestions welcome!

## License

[Your chosen license]

---

## FAQ

### Q: How is this different from Tokio's built-in utilities?

**A**: This library provides higher-level ergonomic patterns inspired by UniTask, while Tokio focuses on the runtime and low-level primitives. We build on top of Tokio to provide common patterns like manual task completion, object pooling, and specialized future types.

### Q: Should I use CompletedTask or just return the value directly?

**A**: Use `CompletedTask` when you need to return a `Future` type from a function, but have an immediate result. It's more efficient than `Box::pin(async { value })`.

### Q: Is TaskCompletionSource thread-safe?

**A**: Yes! You can clone it and complete it from any thread. The future will be properly notified.

### Q: Why not use channels instead of TaskCompletionSource?

**A**: TaskCompletionSource is designed for **single-value, one-time completion**. Channels are for streams of values. TaskCompletionSource is more efficient for the one-shot case and has better ergonomics for bridging callback APIs.

### Q: When should I use ObjectPool?

**A**: Use ObjectPool when you're frequently allocating and deallocating the same type of object (like buffers or connections) in performance-critical code. Profile first - premature optimization is the root of all evil!

### Q: Can I use WhenAny for timeout patterns?

**A**: Yes! Combine it with `Delay` to create timeout operations. However, `tokio::select!` is often more ergonomic for this use case.

### Q: Why does TaskCompletionSource require Clone on the result type?

**A**: To support multiple futures awaiting the same result. Each future gets a cloned copy of the result. If you only need one consumer, consider using channels instead.