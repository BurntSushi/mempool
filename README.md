mempool
=======
A fast thread safe memory pool for reusing allocations.

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
mempool = "0.1"
```

### Benchmarks

```
test bench::mempool_get_put   ... bench:          27 ns/iter (+/- 0)
test bench::mpmc_get_put      ... bench:          32 ns/iter (+/- 0)
test bench::mutex_get_put     ... bench:          45 ns/iter (+/- 0)
test bench::refcell_get_put   ... bench:          17 ns/iter (+/- 0)
test bench::treiber_get_put   ... bench:          95 ns/iter (+/- 1)
```

### Motivation

I needed a very fast way to reuse allocations across multiple threads,
potentially optimizing single threaded use over multithreaded use.

### Future work

The current implementation uses a spin lock, which assumes there is very little
contention.
