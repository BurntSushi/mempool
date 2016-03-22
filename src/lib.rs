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
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

/// The type of an initialization function.
pub type CreateFn<T> = Box<Fn() -> T + Send + Sync + 'static>;

/// A fast memory pool.
pub struct Pool<T: Send + 'static> {
    stack: Stack<T>,
    create: CreateFn<T>,
}

unsafe impl<T: Send + 'static> Sync for Pool<T> {}

/// A guard for putting values back into the pool on drop.
#[derive(Debug)]
pub struct PoolGuard<'a, T: Send + 'static> {
    pool: &'a Pool<T>,
    data: Option<T>,
}

impl<T: Send + 'static> Pool<T> {
    /// Create a new memory pool with the given initialization function.
    pub fn new(create: CreateFn<T>) -> Pool<T> {
        Pool {
            stack: Stack::new(),
            create: create,
        }
    }

    /// Get a new value from the pool.
    ///
    /// If one does not exist, then it is created with the initialization
    /// function.
    pub fn get(&self) -> PoolGuard<T> {
        match self.stack.pop() {
            None => PoolGuard { pool: self, data: Some((self.create)()) },
            Some(data) => PoolGuard { pool: self, data: Some(data) },
        }
    }

    /// Puts a new value into the pool.
    fn put(&self, data: T) {
        self.stack.push(data);
    }
}

impl<'a, T: Send + 'static> Drop for PoolGuard<'a, T> {
    fn drop(&mut self) {
        let data = self.data.take().unwrap();
        self.pool.put(data);
    }
}

impl<'a, T: Send + 'static> ops::Deref for PoolGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data.as_ref().unwrap() }
}

impl<'a, T: Send + 'static> ops::DerefMut for PoolGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { self.data.as_mut().unwrap() }
}

struct Stack<T> {
    stack: UnsafeCell<Vec<T>>,
    lock: SpinLock,
}

impl<T> Stack<T> {
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

impl<T: fmt::Debug + Send + 'static> fmt::Debug for Pool<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pool(...)")
    }
}

impl<T: fmt::Debug> fmt::Debug for Stack<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Stack")
         .field("stack", &"...")
         .field("lock", &self.lock)
         .finish()
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
}
