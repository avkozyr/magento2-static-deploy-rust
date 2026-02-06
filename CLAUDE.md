# Magento 2 Static Deploy - Rust

High-performance static content deployment tool for Magento 2.

---

## Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Throughput | 50k ops/sec | `cargo bench` |
| Latency p99 | < 5ms | Histogram metrics |
| Memory | < 256MB peak | `heaptrack` |
| CPU | 90%+ utilization | `perf stat` |

---

## Architecture Decisions

- **Async Runtime**: Tokio (multi-threaded) for I/O-bound work
- **Parallelism**: Rayon for CPU-bound batch processing
- **Allocator**: jemalloc for reduced fragmentation
- **Serialization**: Zero-copy with rkyv or cap'n proto

---

## Code Standards

### Must Follow

```rust
// ✅ Always propagate errors with context
fn process(path: &Path) -> Result<Data> {
    fs::read(path).context("failed to read input")?
}

// ✅ Pre-allocate collections
let mut results = Vec::with_capacity(expected_size);

// ✅ Prefer borrowing
fn transform(data: &[u8]) -> Result<Output>

// ✅ Document unsafe with SAFETY comments
// SAFETY: Buffer is guaranteed valid for 'a lifetime
unsafe { ... }
```

### Must Avoid

```rust
// ❌ No unwrap/expect in production paths
data.unwrap()

// ❌ No allocations in hot loops
for item in items {
    let s = format!("{}", item);  // Allocates each iteration
}

// ❌ No blocking in async context
async fn bad() {
    std::fs::read(path)  // Blocks runtime
}

// ❌ No Arc<Mutex<T>> when channels work
Arc::new(Mutex::new(Vec::new()))
```

---

## Concurrency Patterns

```rust
// CPU-bound: Rayon
items.par_iter().map(process).collect()

// I/O-bound: Tokio + buffered streams
stream::iter(items)
    .map(|i| async move { fetch(i).await })
    .buffer_unordered(100)
    .collect()

// Progress tracking: Atomics
static COUNTER: AtomicU64 = AtomicU64::new(0);
COUNTER.fetch_add(1, Ordering::Relaxed);
```

---

## Memory Optimization

- Use `&str` over `String`, `&[T]` over `Vec<T>`
- Pool buffers with `crossbeam` or custom pool
- Memory-map large files with `memmap2`
- Use `SmallVec<[T; N]>` for small collections
- Intern repeated strings with `Arc<str>` + `DashMap`

---

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "fs", "io-util"] }
rayon = "1.10"
crossbeam-channel = "0.5"
dashmap = "6"
memmap2 = "0.9"
smallvec = "1"
bytes = "1"
anyhow = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

---

## Build Commands

```bash
# Development
cargo check --all-targets
cargo clippy -- -D warnings
cargo fmt --check

# Release build
cargo build --release

# Benchmarks
cargo bench

# Profile
cargo flamegraph --bin app -- <args>
```

---

## Benchmarking Requirements

Before merging performance-critical changes:

1. Run `cargo bench` baseline on main
2. Apply changes
3. Run `cargo bench` comparison
4. Regression > 5% requires justification
5. Include benchmark results in PR

---

## Workflow & Process

- **ALWAYS** create a detailed plan first before making any code changes, and get user approval before proceeding
- **ALWAYS** keep plans minimal, avoid complex changes unless explicitly requested
- User prefers short plans by default - only show detailed technical specifications when explicitly requested
- Work task-by-task with status updates
- Run `cargo clippy` and `cargo fmt` after each change
- Profile before and after optimization work
- Document performance tradeoffs in commits
- Ask for clarification before implementing unclear requirements

---

## Testing Strategy

- Maintain 80% test coverage across library modules
- Run `cargo test` before committing changes
- Run `cargo tarpaulin --lib` to verify coverage
- Focus on unit tests for public functions and error paths

---

## Copying/Porting Functionality

When copying/porting functionality from any reference implementation:

- **ALWAYS** compare the complete implementation line-by-line
- Check all parameters, method calls, and logic flow
- Consider Rust-specific optimizations (zero-copy, SIMD)
- Benchmark both implementations to verify performance

## Active Technologies

- Rust 1.80+ with rayon 1.11, quick-xml 0.37, walkdir 2, anyhow 1, thiserror 2, clap 4
- 140 unit tests with 84% library coverage
- Criterion benchmarks for performance validation

## Performance Achieved

| Metric | Result |
|--------|--------|
| Single theme | 2,400 files/sec |
| Multi-theme parallel | 5,600 files/sec |
| vs Go | 1.6x faster |
| vs PHP | 12x faster |
| Binary size | 935 KB |
