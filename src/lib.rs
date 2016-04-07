/*!
This crate provides a fast thread safe memory pool for reusing allocations. It
aggressively optimizes for the single-threaded use case, but gracefully
supports access from multiple threads simultaneously. In particular, values in
a pool may not be shared across threads.

# Example

A pool takes an initialization function for creating members of the pool. Once
created, values can be immediately retrieved.

```rust
use mempool::Pool;

let pool = Pool::new(Box::new(|| "foobar"));
assert_eq!("foobar", *pool.get());
```

Note that the pool returns an immutable reference. If you need a mutable
reference, then use a `RefCell`. (Which is guaranteed safe by the pool.)
*/
#![deny(missing_docs)]
#![cfg_attr(feature = "nightly", feature(test))]

use std::collections::hash_map::{HashMap, Entry};
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
use std::sync::atomic::Ordering::Relaxed;

// A counter provides the basis for assigning an id to each thread that tries
// to access the pool. In particular, the first thread to access a pool becomes
// its owner, and correspondingly is the only thread with access to the "fast"
// path.
//
// The thread id `0` is a special sentinel value to indicate that the pool has
// no owner yet. Therefore, all thread ids assigned to a thread start from `1`.
static COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;
thread_local!(static THREAD_ID: usize = COUNTER.fetch_add(1, Relaxed) + 1);

/// The type of an initialization function.
pub type CreateFn<T> = Box<Fn() -> T + Send + Sync + 'static>;

/// A fast memory pool.
pub struct Pool<T: Send> {
    create: CreateFn<T>,
    owner: AtomicUsize,
    owner_val: T,
    global: Mutex<HashMap<usize, Box<T>>>,
}

unsafe impl<T: Send> Sync for Pool<T> {}

impl<T: fmt::Debug + Send + 'static> fmt::Debug for Pool<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pool({:?})", self.owner_val)
    }
}

impl<T: Send> Pool<T> {
    /// Create a new memory pool with the given initialization function.
    pub fn new(create: CreateFn<T>) -> Pool<T> {
        let owner_val = (create)();
        Pool {
            create: create,
            owner: AtomicUsize::new(0),
            owner_val: owner_val,
            global: Mutex::new(HashMap::new()),
        }
    }

    /// Get a reference to a new value from the pool. The underlying value may
    /// be reused in subsequent calls to `get`.
    ///
    /// If one does not exist, then it is created with the initialization
    /// function.
    // The inline(always) here seems necessary to get this function to inline,
    // which saves quite a few cycles. (And seems appropriate, since the whole
    // point here is to reduce overhead.) It's good for about 3x improvement
    // in the mempool_get_put_tls benchmark.
    #[inline(always)]
    pub fn get(&self) -> &T {
        let id = THREAD_ID.with(|id| *id);
        let owner = self.owner.load(Relaxed);
        // If the owner has already been assigned and this thread is the owner,
        // then just return a reference to the owner's cache.
        if owner == id {
            return &self.owner_val;
        }
        self.get_slow(owner, id)
    }

    #[cold]
    fn get_slow(&self, owner: usize, thread_id: usize) -> &T {
        if owner == 0 {
            if self.owner.compare_and_swap(0, thread_id, Relaxed) == 0 {
                return &self.owner_val;
            }
        }
        let mut global = self.global.lock().unwrap();
        match global.entry(thread_id) {
            Entry::Occupied(ref e) => {
                let p: *const T = &**e.get();
                unsafe { &*p }
            }
            Entry::Vacant(e) => {
                let t = Box::new((self.create)());
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
    use std::cell::RefCell;
    use std::sync::Arc;
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
        // This tests that multiple accesses from the same thread don't create
        // new values.
        let pool = Pool::new(dummy());
        {
            assert_eq!(&Dummy(0), &*pool.get());
        }
        assert_eq!(&Dummy(0), &*pool.get());
        assert_eq!(&Dummy(0), &*pool.get());
    }

    #[test]
    fn no_reuse() {
        // This tests that a pool's values aren't shared between threads.
        // i.e., the init function is called when another thread tries to
        // get a value.
        let pool = Arc::new(Pool::new(dummy()));
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
        foo::<Pool<String>>();
        foo::<Pool<RefCell<String>>>();
    }
}
