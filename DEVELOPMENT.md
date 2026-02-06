# Development Guide

## Project Structure

```
src/
├── lib.rs       # Library exports
├── main.rs      # CLI entry, orchestration, Rayon execution
├── config.rs    # Clap CLI parsing, Config struct
├── theme.rs     # Theme struct, XML parsing, Hyva detection (30 tests)
├── scanner.rs   # Theme discovery, file source scanning (28 tests)
├── deployer.rs  # Deploy jobs, parallel execution, Luma delegation (31 tests)
├── copier.rs    # File copy with cancellation support (27 tests)
└── error.rs     # Error types with thiserror (11 tests)

benches/
└── deploy_benchmark.rs  # Criterion micro-benchmarks
```

## Building

```bash
# Debug build
cargo build

# Release build (optimized, ~900KB binary)
cargo build --release

# Check without building
cargo check
```

## Code Quality

```bash
# Lint with clippy (warnings as errors)
cargo clippy -- -D warnings

# Format code
cargo fmt

# Both
cargo fmt && cargo clippy -- -D warnings
```

## Architecture

### Data Flow

```
CLI Args → Config → discover_themes() → job_matrix() → par_iter() → deploy_theme() → Result
```

### Key Components

**Config** (`config.rs`)
- Parses CLI with clap derive macros
- Validates magento_root exists
- Sets parallelism from --jobs

**Theme** (`theme.rs`)
- Parses theme.xml for parent chain
- Detects Hyva vs Luma themes
- Resolves inheritance order

**Scanner** (`scanner.rs`)
- Discovers themes in app/design/
- Collects file sources with priority order
- Scans vendor modules and lib/web

**Deployer** (`deployer.rs`)
- Creates job matrix (theme × locale)
- Parallel execution with Rayon
- Luma delegation to bin/magento
- Collects results and stats

**Copier** (`copier.rs`)
- File copy with std::fs::copy (zero-copy)
- Directory walking with walkdir
- Cancellation check in loops

### Parallelism

Uses Rayon's work-stealing thread pool:

```rust
jobs.par_iter()
    .map(|job| deploy_theme(job, ...))
    .collect()
```

### Cancellation

AtomicBool shutdown flag checked in copy loops:

```rust
if shutdown.load(Ordering::Relaxed) {
    return Err(DeployError::Cancelled);
}
```

### Error Handling

- `anyhow` for application errors with context
- `thiserror` for typed error variants
- No `.unwrap()` in production paths

## Performance Guidelines

From constitution.md:

1. **No allocations in hot paths**
   - Use `&str` over `String`
   - Pre-allocate with `with_capacity()`

2. **Zero-copy file operations**
   - `std::fs::copy` uses kernel zero-copy
   - No buffered reading/writing

3. **Atomic counters**
   - `AtomicU64` for stats, not mutexes
   - `Ordering::Relaxed` for counters

4. **Rayon for CPU-bound work**
   - Work-stealing balances load
   - Configure with `ThreadPoolBuilder`

## Release Profile

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

## Benchmarking

The project uses [Criterion](https://github.com/bheisler/criterion.rs) for micro-benchmarks.

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench -- copy_file

# Save baseline for comparison
cargo bench -- --save-baseline before

# Compare against baseline (after changes)
cargo bench -- --baseline before
```

### Available Benchmarks

| Benchmark | Description |
|-----------|-------------|
| `copy_file_1kb` | Single 1KB file copy |
| `copy_file_1mb` | Single 1MB file copy |
| `copy_directory/100` | Copy 100 files |
| `copy_directory/500` | Copy 500 files |
| `copy_directory/1000` | Copy 1000 files |
| `discover_themes_5` | Discover 5 mock themes |

### Benchmark Results (Reference)

Results on Apple M2 Pro with NVMe SSD:

| Benchmark | Time | Throughput |
|-----------|------|------------|
| copy_file_1kb | 182 µs | - |
| copy_file_1mb | 160 µs | - |
| copy_directory/100 | 35.7 ms | 2,803 files/sec |
| copy_directory/500 | 128.4 ms | 3,894 files/sec |
| copy_directory/1000 | 259.5 ms | 3,854 files/sec |
| discover_themes_5 | 199 µs | - |

### Real-World Benchmarks

Tested on real Magento 2 projects (Life/default theme, nl_NL locale):

| Tool | Time | Files | Speed | vs PHP |
|------|------|-------|-------|--------|
| **Rust** | **1.47s** | 3,519 | **2,400 files/sec** | **12x faster** |
| Go | 2.37s | 3,479 | 1,481 files/sec | 7x faster |
| PHP | 17.56s | 10,342 | ~589 files/sec | baseline |

Multi-theme parallel deployment: **5,600+ files/sec**

```bash
# Clean previous output
rm -rf /var/www/magento/pub/static/frontend/Vendor/Theme/en_US

# Run with timing
./target/release/magento-static-deploy -v /var/www/magento --theme Vendor/Theme
```

### Profiling

```bash
# Install flamegraph
cargo install flamegraph

# Profile (requires sudo on Linux)
sudo flamegraph -- ./target/release/magento-static-deploy /var/www/magento

# View flamegraph.svg in browser
```

## Testing Against Magento

```bash
# Build release
cargo build --release

# Test with Magento installation
./target/release/magento-static-deploy -v /var/www/magento

# Expected output
Deployed 8,234 files in 0.21s (39,209 files/sec)
  frontend/Vendor/Hyva/en_US: 8,234 files
```

## Debugging

```bash
# Verbose output
magento-static-deploy -v /var/www/magento

# Check theme detection
ls /var/www/magento/app/design/frontend/

# Verify theme.xml
cat /var/www/magento/app/design/frontend/Vendor/Hyva/theme.xml
```

## Testing

```bash
# Run all tests (140 tests)
cargo test

# Run specific module tests
cargo test theme::
cargo test copier::

# Check test coverage (requires cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --lib

# Current coverage: 84% (394/468 lines)
```

## Contributing

1. Follow constitution.md principles
2. Run `cargo clippy -- -D warnings` and `cargo fmt` before commits
3. Maintain 80% test coverage
4. Profile before/after optimization changes
5. Include benchmark results for performance changes
