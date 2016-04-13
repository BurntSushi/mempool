**THIS CRATE IS DEPRECATED**. Instead, you should prefer the
[`thread_local`](https://github.com/Amanieu/thread_local-rs) crate. In
particular, the `CachedThreadLocal` should replace the pool in this crate
(still optimizing for the single thread case, but also being much faster in
the multithreaded case).


mempool
=======
This crate provides a fast thread safe memory pool for reusing allocations. It
aggressively optimizes for the single-threaded use case, but gracefully
supports access from multiple threads simultaneously. In particular, values in
a pool may not be shared across threads.

[![Linux build status](https://api.travis-ci.org/BurntSushi/mempool.png)](https://travis-ci.org/BurntSushi/mempool)
[![Windows build status](https://ci.appveyor.com/api/projects/status/github/BurntSushi/mempool?svg=true)](https://ci.appveyor.com/project/BurntSushi/mempool)
[![](http://meritbadge.herokuapp.com/mempool)](https://crates.io/crates/mempool)

Dual-licensed under MIT or the [UNLICENSE](http://unlicense.org).

### Documentation

[http://burntsushi.net/rustdoc/mempool/](http://burntsushi.net/rustdoc/mempool/)

### Usage

To use this crate, add `mempool` as a dependency to your project's
`Cargo.toml`:

```
[dependencies]
mempool = "0.3"
```

### Benchmarks

This crate currently uses the `mempool_get_put_tls` approach.

```
test bench::crossbeam_ms_get_put      ... bench:         105 ns/iter (+/- 4)
test bench::crossbeam_seg_get_put     ... bench:          87 ns/iter (+/- 25)
test bench::crossbeam_treiber_get_put ... bench:          93 ns/iter (+/- 1)
test bench::mempool_get_put_tls       ... bench:           1 ns/iter (+/- 0)
test bench::mpmc_get_put              ... bench:          30 ns/iter (+/- 0)
test bench::mutex_get_put             ... bench:          46 ns/iter (+/- 0)
```

### Motivation

I needed a very fast way to reuse allocations across multiple threads,
potentially optimizing single threaded use over multithreaded use.

### Future work

The current implementation is very fast for single threaded use, but probably
slower than it needs to be for multithreaded use.
