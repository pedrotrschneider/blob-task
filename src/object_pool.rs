use std::sync::{Arc, Mutex};

/// A thread-safe object pool for reusing allocations.
/// Reduces memory allocations by recycling objects.
pub struct ObjectPool<T, F>
where
    F: Fn() -> T,
{
    objects: Arc<Mutex<Vec<T>>>,
    factory: F,
    max_capacity: Option<usize>,
}

impl<T, F> Clone for ObjectPool<T, F>
where
    F: Fn() -> T + Clone,
{
    fn clone(&self) -> Self {
        return Self {
            objects: Arc::clone(&self.objects),
            factory: self.factory.clone(),
            max_capacity: self.max_capacity,
        };
    }
}

impl<T, F> ObjectPool<T, F>
where
    F: Fn() -> T + Clone,
{
    /// Creates a new object pool with a factory function
    pub fn new(factory: F) -> Self {
        return Self {
            objects: Arc::new(Mutex::new(Vec::new())),
            factory,
            max_capacity: None,
        };
    }

    /// Creates a new object pool with a factory function and maximum capacity
    pub fn with_capacity(factory: F, max_capacity: usize) -> Self {
        return Self {
            objects: Arc::new(Mutex::new(Vec::with_capacity(max_capacity))),
            factory,
            max_capacity: Some(max_capacity),
        };
    }

    /// Rents an object from the pool. If the pool is empty, creates a new object.
    pub fn rent(&self) -> PooledObject<T, F> {
        let obj = {
            let mut objects = self.objects.lock().unwrap();
            objects.pop()
        };

        let obj = obj.unwrap_or_else(|| (self.factory)());

        return PooledObject {
            object: Some(obj),
            pool: self.clone(),
        };
    }

    /// Returns an object to the pool
    fn return_object(&self, obj: T) {
        let mut objects = self.objects.lock().unwrap();

        // Only store if under capacity limit
        if let Some(max) = self.max_capacity {
            if objects.len() < max {
                objects.push(obj);
            }
            // Otherwise, drop the object
        } else {
            objects.push(obj);
        }
    }

    /// Returns the current number of available objects in the pool
    pub fn available(&self) -> usize {
        return self.objects.lock().unwrap().len();
    }

    /// Clears all objects from the pool
    pub fn clear(&self) {
        self.objects.lock().unwrap().clear();
    }
}

/// A pooled object that automatically returns to the pool when dropped.
/// Provides RAII-style resource management.
pub struct PooledObject<T, F>
where
    F: Fn() -> T + Clone,
{
    object: Option<T>,
    pool: ObjectPool<T, F>,
}

impl<T, F> PooledObject<T, F>
where
    F: Fn() -> T + Clone,
{
    /// Takes ownership of the inner object, preventing it from being returned to the pool
    pub fn take(mut self) -> T {
        return self.object.take().expect("Object already taken");
    }
}

impl<T, F> std::ops::Deref for PooledObject<T, F>
where
    F: Fn() -> T + Clone,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        return self.object.as_ref().expect("Object already taken");
    }
}

impl<T, F> std::ops::DerefMut for PooledObject<T, F>
where
    F: Fn() -> T + Clone,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        return self.object.as_mut().expect("Object already taken");
    }
}

impl<T, F> Drop for PooledObject<T, F>
where
    F: Fn() -> T + Clone,
{
    fn drop(&mut self) {
        if let Some(obj) = self.object.take() {
            self.pool.return_object(obj);
        }
    }
}

/// ObjectPool with a reset function that cleans objects before returning them to the pool
pub struct ObjectPoolWithReset<T, F, R>
where
    F: Fn() -> T,
    R: Fn(&mut T),
{
    objects: Arc<Mutex<Vec<T>>>,
    factory: F,
    reset: R,
    max_capacity: Option<usize>,
}

impl<T, F, R> Clone for ObjectPoolWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    fn clone(&self) -> Self {
        return Self {
            objects: Arc::clone(&self.objects),
            factory: self.factory.clone(),
            reset: self.reset.clone(),
            max_capacity: self.max_capacity,
        };
    }
}

impl<T, F, R> ObjectPoolWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    /// Creates a new object pool with factory and reset functions
    pub fn new(factory: F, reset: R) -> Self {
        return Self {
            objects: Arc::new(Mutex::new(Vec::new())),
            factory,
            reset,
            max_capacity: None,
        };
    }

    /// Creates a new object pool with factory, reset functions, and maximum capacity
    pub fn with_capacity(factory: F, reset: R, max_capacity: usize) -> Self {
        return Self {
            objects: Arc::new(Mutex::new(Vec::with_capacity(max_capacity))),
            factory,
            reset,
            max_capacity: Some(max_capacity),
        };
    }

    /// Rents an object from the pool
    pub fn rent(&self) -> PooledObjectWithReset<T, F, R> {
        let obj = {
            let mut objects = self.objects.lock().unwrap();
            objects.pop()
        };

        let obj = obj.unwrap_or_else(|| (self.factory)());

        return PooledObjectWithReset {
            object: Some(obj),
            pool: self.clone(),
        };
    }

    /// Returns an object to the pool after resetting it
    fn return_object(&self, mut obj: T) {
        (self.reset)(&mut obj);

        let mut objects = self.objects.lock().unwrap();

        if let Some(max) = self.max_capacity {
            if objects.len() < max {
                objects.push(obj);
            }
        } else {
            objects.push(obj);
        }
    }

    /// Returns the current number of available objects in the pool
    pub fn available(&self) -> usize {
        return self.objects.lock().unwrap().len();
    }

    /// Clears all objects from the pool
    pub fn clear(&self) {
        self.objects.lock().unwrap().clear();
    }
}

/// A pooled object with reset functionality
pub struct PooledObjectWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    object: Option<T>,
    pool: ObjectPoolWithReset<T, F, R>,
}

impl<T, F, R> PooledObjectWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    /// Takes ownership of the inner object, preventing it from being returned to the pool
    pub fn take(mut self) -> T {
        return self.object.take().expect("Object already taken");
    }
}

impl<T, F, R> std::ops::Deref for PooledObjectWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        return self.object.as_ref().expect("Object already taken");
    }
}

impl<T, F, R> std::ops::DerefMut for PooledObjectWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        return self.object.as_mut().expect("Object already taken");
    }
}

impl<T, F, R> Drop for PooledObjectWithReset<T, F, R>
where
    F: Fn() -> T + Clone,
    R: Fn(&mut T) + Clone,
{
    fn drop(&mut self) {
        if let Some(obj) = self.object.take() {
            self.pool.return_object(obj);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn basic_pool_rent_and_return() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        {
            let mut obj = pool.rent();
            obj.push(1);
            obj.push(2);
            
            assert_eq!(obj.len(), 2);
            assert_eq!(obj.get(0), Some(&1));
            assert_eq!(obj.get(1), Some(&2));
        } // obj is automatically returned to pool

        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn pool_creates_on_empty() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let create_count_clone = Arc::clone(&create_count);

        let pool = ObjectPool::new(move || {
            create_count_clone.fetch_add(1, Ordering::SeqCst);
            return Vec::<i32>::new();
        });

        let _obj1 = pool.rent();
        let _obj2 = pool.rent();

        assert_eq!(create_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn pool_reuses_objects() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let create_count_clone = Arc::clone(&create_count);

        let pool = ObjectPool::new(move || {
            create_count_clone.fetch_add(1, Ordering::SeqCst);
            return Vec::<i32>::new();
        });

        {
            let _obj = pool.rent();
        } // Return to pool

        let _obj = pool.rent(); // Should reuse

        assert_eq!(create_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pool_with_capacity_limit() {
        let pool = ObjectPool::with_capacity(|| Vec::<i32>::new(), 2);

        {
            let _obj1 = pool.rent();
            let _obj2 = pool.rent();
            let _obj3 = pool.rent();
        } // All return

        // Should only keep 2
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn pool_available_count() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        assert_eq!(pool.available(), 0);

        {
            let _obj = pool.rent();
            assert_eq!(pool.available(), 0);
        }

        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn pool_clear() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        {
            let _obj1 = pool.rent();
            let _obj2 = pool.rent();
        }

        assert_eq!(pool.available(), 2);

        pool.clear();

        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn pooled_object_deref() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        let mut obj = pool.rent();
        obj.push(1);
        obj.push(2);

        assert_eq!(obj.len(), 2);
        assert_eq!(obj[0], 1);
    }

    #[test]
    fn pooled_object_take() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        let mut obj = pool.rent();
        obj.push(1);

        let vec = obj.take();

        assert_eq!(vec.len(), 1);
        assert_eq!(pool.available(), 0); // Not returned to pool
    }

    #[test]
    fn pool_with_reset() {
        let pool = ObjectPoolWithReset::new(
            || Vec::<i32>::new(),
            |v| v.clear(), // Reset function
        );

        {
            let mut obj = pool.rent();
            obj.push(1);
            obj.push(2);
        } // Should reset and return

        let obj = pool.rent();
        assert_eq!(obj.len(), 0); // Should be empty after reset
    }

    #[test]
    fn pool_with_reset_capacity() {
        let pool = ObjectPoolWithReset::with_capacity(|| Vec::<i32>::new(), |v| v.clear(), 2);

        {
            let _obj1 = pool.rent();
            let _obj2 = pool.rent();
            let _obj3 = pool.rent();
        }

        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn pool_clone_shares_state() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        {
            let _obj = pool.rent();
        }

        let pool_clone = pool.clone();

        assert_eq!(pool_clone.available(), 1);
    }

    #[test]
    fn pool_with_reset_clone() {
        let pool = ObjectPoolWithReset::new(|| Vec::<i32>::new(), |v| v.clear());

        {
            let _obj = pool.rent();
        }

        let pool_clone = pool.clone();

        assert_eq!(pool_clone.available(), 1);
    }

    #[tokio::test]
    async fn pool_concurrent_access() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        // Create scoped block to ensure objects are dropped
        {
            let pool_clone = pool.clone();
            let task1 = tokio::spawn(async move {
                let mut obj = pool_clone.rent();
                obj.push(1);
            });

            let pool_clone = pool.clone();
            let task2 = tokio::spawn(async move {
                let mut obj = pool_clone.rent();
                obj.push(2);
            });

            task1.await.unwrap();
            task2.await.unwrap();
        }

        // After the scope, both objects should be returned
        // They might reuse the same object or create new ones depending on timing
        assert!(pool.available() >= 1 && pool.available() <= 2);
    }

    #[test]
    fn pool_with_custom_type() {
        #[derive(Default)]
        struct Buffer {
            data: Vec<u8>,
        }

        let pool = ObjectPool::new(|| Buffer::default());

        let mut obj = pool.rent();
        obj.data.extend_from_slice(&[1, 2, 3]);

        assert_eq!(obj.data.len(), 3);
    }

    #[test]
    fn pool_multiple_rent_cycles() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        for _ in 0..10 {
            let mut obj = pool.rent();
            obj.push(1);
        }

        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn pool_with_reset_multiple_cycles() {
        let pool = ObjectPoolWithReset::new(|| Vec::<i32>::new(), |v| v.clear());

        for i in 0..5 {
            let mut obj = pool.rent();
            obj.push(i);
            assert_eq!(obj.len(), 1);
        }

        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn pooled_object_with_reset_take() {
        let pool = ObjectPoolWithReset::new(|| Vec::<i32>::new(), |v| v.clear());

        let mut obj = pool.rent();
        obj.push(1);

        let vec = obj.take();

        assert_eq!(vec.len(), 1);
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn pool_zero_capacity() {
        let pool = ObjectPool::with_capacity(|| Vec::<i32>::new(), 0);

        {
            let _obj = pool.rent();
        }

        assert_eq!(pool.available(), 0);
    }

    #[tokio::test]
    async fn pool_high_contention() {
        let pool = ObjectPool::new(|| Vec::<i32>::new());

        let mut handles = vec![];

        for i in 0..100 {
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                let mut obj = pool_clone.rent();
                obj.push(i);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Some objects should be in the pool
        assert!(pool.available() > 0);
    }

    #[test]
    fn pool_with_reset_clear() {
        let pool = ObjectPoolWithReset::new(|| Vec::<i32>::new(), |v| v.clear());

        {
            let _obj1 = pool.rent();
            let _obj2 = pool.rent();
            let _obj3 = pool.rent();
        }

        assert_eq!(pool.available(), 3);

        pool.clear();

        assert_eq!(pool.available(), 0);
    }
}
