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
#![allow(dead_code, unused_imports)]
#![deny(missing_docs)]
#![cfg_attr(feature = "nightly", feature(test))]

use std::cell::UnsafeCell;
use std::collections::hash_map::{HashMap, Entry};
use std::fmt;
use std::ops;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, ATOMIC_USIZE_INIT};
use std::sync::atomic::Ordering;

static THREAD_ID_COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;
thread_local!(
    static THREAD_ID: usize =
        THREAD_ID_COUNTER.fetch_add(1, Ordering::Relaxed) + 1
);

/// The type of an initialization function.
pub type CreateFn<T> = Box<Fn() -> T + Send + 'static>;

/// A fast memory pool.
pub struct Pool<T: Send + 'static>(Arc<PoolInner<T>>);

unsafe impl<T: Send + 'static> Sync for PoolInner<T> {}

struct PoolInner<T: Send + 'static> {
    owner: AtomicUsize,
    create: CreateFn<T>,
    local: T,
    global: Mutex<HashMap<usize, Box<T>>>,
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

impl<T: Send + 'static> Pool<T> {
    /// Create a new memory pool with the given initialization function.
    pub fn new(create: CreateFn<T>) -> Pool<T> {
        let local = (create)();
        Pool(Arc::new(PoolInner {
            owner: AtomicUsize::new(0),
            create: create,
            local: local,
            global: Mutex::new(HashMap::new()),
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
    pub fn get(&self) -> &T {
        let id = THREAD_ID.with(|id| *id);
        let owner = self.0.owner.load(Ordering::Relaxed);
        if owner == id {
            return &self.0.local;
        }
        self.get_slow(owner, id)
    }

    #[cold]
    fn get_slow(&self, owner: usize, thread_id: usize) -> &T {
        if owner == 0 {
            if self.0.owner.compare_and_swap(0, thread_id, Ordering::Relaxed) == 0 {
                return &self.0.local;
            }
        }
        let mut global = self.0.global.lock().unwrap();
        match global.entry(thread_id) {
            Entry::Occupied(ref e) => {
                let p: *const T = &**e.get();
                unsafe { &*p }
            }
            Entry::Vacant(e) => {
                let t = Box::new((self.0.create)());
                let p: *const T = &*t;
                e.insert(t);
                unsafe { &*p }
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "nightly")]
mod bench;

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;
    use std::thread;

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

        let pool2 = pool.clone();
        thread::spawn(move || {
            assert_eq!(&Dummy(1), &*pool2.get());
        }).join().unwrap();
    }

    #[test]
    fn is_sync() {
        fn foo<T: Sync>() {}
        foo::<Pool<String>>()
    }
}
