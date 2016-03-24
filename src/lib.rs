/*!
This crate provides a fast thread safe memory pool for reusing allocations.

# Example

A pool takes an initialization function for creating members of the pool. Once
created, values can be immediately retrieved. Once the value is dropped, it is
returned to the pool for reuse.

```rust
use mempool::Pool;

let pool = Pool::new(Box::new(|| "foobar"));
assert_eq!("foobar", *pool.get());
```
*/
#![deny(missing_docs)]
#![cfg_attr(feature = "nightly", feature(test))]

use std::cell::UnsafeCell;
use std::fmt;
use std::ops;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

/// The type of an initialization function.
pub type CreateFn<T> = Box<Fn() -> T + Send + Sync + 'static>;

/// A fast memory pool.
pub struct Pool<T: Send + 'static>(Arc<_Pool<T>>);

struct _Pool<T: Send + 'static> {
    stack: Stack<T>,
    create: CreateFn<T>,
}

impl<T: Send + 'static> Clone for Pool<T> {
    fn clone(&self) -> Pool<T> {
        Pool(self.0.clone())
    }
}

impl<T: fmt::Debug + Send + 'static> fmt::Debug for Pool<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pool(...)")
    }
}

/// A guard for putting values back into the pool on drop.
///
/// This stores a borrowed reference to the pool that it originated from.
#[derive(Debug)]
pub struct RefGuard<'a, T: Send + 'static> {
    pool: &'a Pool<T>,
    data: Option<T>,
}

/// A guard for putting values back into the pool on drop.
#[derive(Debug)]
pub struct Guard<T: Send + 'static> {
    pool: Pool<T>,
    data: Option<T>,
}

impl<T: Send + 'static> Pool<T> {
    /// Create a new memory pool with the given initialization function.
    pub fn new(create: CreateFn<T>) -> Pool<T> {
        Pool(Arc::new(_Pool {
            stack: Stack::new(),
            create: create,
        }))
    }

    /// Get a new value from the pool.
    ///
    /// If one does not exist, then it is created with the initialization
    /// function.
    ///
    /// This returns a guard without any lifetime variables. In exchange, it
    /// has slightly larger overhead than `get_ref`.
    ///
    /// When the guard is dropped, the underlying value is returned to the
    /// pool.
    pub fn get(&self) -> Guard<T> {
        match self.0.stack.pop() {
            None => Guard { pool: self.clone(), data: Some((self.0.create)()) },
            Some(data) => Guard { pool: self.clone(), data: Some(data) },
        }
    }

    /// Get a new value from the pool.
    ///
    /// If one does not exist, then it is created with the initialization
    /// function.
    ///
    /// This returns a guard with a borrowed reference to the underlying pool.
    ///
    /// When the guard is dropped, the underlying value is returned to the
    /// pool.
    pub fn get_ref(&self) -> RefGuard<T> {
        match self.0.stack.pop() {
            None => RefGuard { pool: self, data: Some((self.0.create)()) },
            Some(data) => RefGuard { pool: self, data: Some(data) },
        }
    }

    /// Puts a new value into the pool.
    fn put(&self, data: T) {
        self.0.stack.push(data);
    }
}

impl<'a, T: Send + 'static> Drop for RefGuard<'a, T> {
    fn drop(&mut self) {
        let data = self.data.take().unwrap();
        self.pool.put(data);
    }
}

impl<'a, T: Send + 'static> ops::Deref for RefGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data.as_ref().unwrap() }
}

impl<'a, T: Send + 'static> ops::DerefMut for RefGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { self.data.as_mut().unwrap() }
}

impl<T: Send + 'static> Drop for Guard<T> {
    fn drop(&mut self) {
        let data = self.data.take().unwrap();
        self.pool.put(data);
    }
}

impl<T: Send + 'static> ops::Deref for Guard<T> {
    type Target = T;

    fn deref(&self) -> &T { self.data.as_ref().unwrap() }
}

impl<T: Send + 'static> ops::DerefMut for Guard<T> {
    fn deref_mut(&mut self) -> &mut T { self.data.as_mut().unwrap() }
}

struct Stack<T: Send + 'static> {
    stack: UnsafeCell<Vec<T>>,
    lock: SpinLock,
}

unsafe impl<T: Send + 'static> Sync for Stack<T> {}

impl<T: Send + 'static> Stack<T> {
    fn new() -> Stack<T> {
        Stack {
            stack: UnsafeCell::new(vec![]),
            lock: SpinLock::new(),
        }
    }

    fn push(&self, data: T) {
        self.lock.lock();
        let mut stack = unsafe { &mut *self.stack.get() };
        stack.push(data);
        self.lock.unlock();
    }

    fn pop(&self) -> Option<T> {
        self.lock.lock();
        let mut stack = unsafe { &mut *self.stack.get() };
        let data = stack.pop();
        self.lock.unlock();
        data
    }
}

impl<T: fmt::Debug + Send + 'static> fmt::Debug for Stack<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Stack")
         .field("stack", &"...")
         .field("lock", &self.lock)
         .finish()
    }
}

#[derive(Debug)]
struct SpinLock {
    locked: AtomicBool,
}

impl SpinLock {
    fn new() -> SpinLock {
        SpinLock { locked: AtomicBool::new(false) }
    }

    fn lock(&self) {
        while self.locked.swap(true, Ordering::Acquire) {}
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::Release)
    }
}

#[cfg(test)]
#[cfg(feature = "nightly")]
mod bench;

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;

    use super::{CreateFn, Pool};

    #[derive(Debug, Eq, PartialEq)]
    struct Dummy(usize);

    fn dummy() -> CreateFn<Dummy> {
        let count = AtomicUsize::new(0);
        Box::new(move || {
            Dummy(count.fetch_add(1, SeqCst))
        })
    }

    #[test]
    fn empty() {
        let pool = Pool::new(dummy());
        assert_eq!(&Dummy(0), &*pool.get());
    }

    #[test]
    fn reuse() {
        let pool = Pool::new(dummy());
        {
            assert_eq!(&Dummy(0), &*pool.get());
        }
        assert_eq!(&Dummy(0), &*pool.get());
    }

    #[test]
    fn no_reuse() {
        let pool = Pool::new(dummy());
        let val = pool.get();
        assert_eq!(&Dummy(0), &*val);
        assert_eq!(&Dummy(1), &*pool.get());
    }

    #[test]
    fn is_sync() {
        fn foo<T: Sync>() {}
        foo::<Pool<String>>()
    }
}
