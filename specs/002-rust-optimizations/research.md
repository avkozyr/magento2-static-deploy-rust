# Research: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2025-02-05

## Technology Decisions

### 1. Path Construction Optimization

**Decision**: Use `PathBuf::join()` chain instead of `format!()`

**Rationale**:
- `format!()` allocates a new String on each call
- `PathBuf::join()` uses efficient path concatenation without intermediate strings
- Zero heap allocations for path construction in hot loops

**Alternatives Considered**:
- `String::with_capacity()` + push_str: Still allocates, just pre-sized
- `Cow<Path>`: Adds complexity without significant benefit

### 2. Inline Hints for Accessors

**Decision**: Add `#[inline]` to small accessor methods

**Rationale**:
- Accessor methods like `as_str()` are called in every file operation
- `#[inline]` hint allows cross-crate inlining
- Compiler already inlines within crate, but explicit hints help benchmarks

**Alternatives Considered**:
- `#[inline(always)]`: Too aggressive, may bloat code
- No hint: Works within crate but not for library users

### 3. XML Parsing Zero-Copy

**Decision**: Use `quick-xml`'s `unescape_value()` API

**Rationale**:
- `to_vec()` copies attribute bytes into new Vec
- `unescape_value()` returns Cow<str>, only allocating if escape sequences present
- Most XML attributes have no escapes, so zero-copy in practice

**Alternatives Considered**:
- `roxmltree`: DOM-based, higher memory usage
- `serde_xml_rs`: Convenient but more allocations

### 4. Rayon Thread Pool Configuration

**Decision**: Configure `num_threads = num_cpus::get() * 2` for I/O-bound work

**Rationale**:
- File operations are I/O-bound, not CPU-bound
- More threads than cores allows overlap during I/O waits
- 2× multiplier is common for I/O-heavy workloads

**Alternatives Considered**:
- Default (num_cpus): Under-utilizes I/O capacity
- 4× multiplier: Diminishing returns, more context switches

### 5. Progress Bar Batching

**Decision**: Thread-local counters with periodic atomic flush

**Rationale**:
- Per-file atomic updates cause cache line contention
- Thread-local accumulation + batch flush reduces atomic operations 100×
- Batch size of 100 files balances accuracy vs overhead

**Alternatives Considered**:
- Per-thread progress bars: Complex UI, more overhead
- No progress: Poor user experience

### 6. Buffer Size for File Copy

**Decision**: Increase buffer to 256KB

**Rationale**:
- Modern SSDs perform better with larger sequential reads/writes
- 256KB aligns well with typical SSD page sizes
- Typical Magento static files are < 100KB, so single-read common

**Alternatives Considered**:
- 64KB (current): Suboptimal for modern storage
- 1MB: Diminishing returns, more memory per thread

### 7. Test Coverage Tool

**Decision**: Use `cargo-tarpaulin` for coverage measurement

**Rationale**:
- Well-maintained, widely used in Rust ecosystem
- Supports line and branch coverage
- Integrates with CI systems
- Faster than `llvm-cov` for typical projects

**Alternatives Considered**:
- `llvm-cov`: More accurate but complex setup
- `grcov`: Requires nightly compiler features
- `kcov`: Linux-only, slower

### 8. Locale Validation

**Decision**: Validate format with `is_valid_format()` method returning bool

**Rationale**:
- Simple regex-free validation: split on '_', check part lengths
- Non-blocking validation (warn, don't error) for maximum compatibility
- Magento supports many locale formats beyond strict ISO

**Alternatives Considered**:
- Strict validation with Result: May break valid Magento setups
- No validation: Loses opportunity to catch typos early

## Performance Baseline

Current metrics (from 001-rust-port validation):
- Throughput: ~10,800 files/second
- Binary size: 919KB
- Memory: Not profiled

Target improvements:
- Throughput: 13,500+ files/second (25% improvement)
- Memory: 20% reduction in allocations
- Binary size: Minimal impact expected

## Test Coverage Strategy

### Unit Test Targets

| Module | Functions to Test | Edge Cases |
|--------|-------------------|------------|
| theme.rs | ThemeCode::new/parse, LocaleCode::new, parse_theme_xml, detect_theme_type | Invalid format, empty strings, malformed XML |
| scanner.rs | discover_themes, scan_*_sources, collect_file_sources | Missing directories, symlinks, permission errors |
| deployer.rs | job_matrix, deploy_theme, output_path_for_theme | Empty themes, cancellation mid-deploy |
| copier.rs | copy_file, copy_directory_with_overrides | Large files, disk full, read-only dest |
| config.rs | Config::from_cli | Missing args, invalid values |
| error.rs | Error Display implementations | All error variants |

### Integration Test Targets

| Scenario | Description |
|----------|-------------|
| Single theme deploy | Deploy one Hyva theme to verify correctness |
| Multi-theme deploy | Deploy multiple themes in parallel |
| Cancellation | Verify graceful shutdown on SIGINT |
| Error recovery | Verify partial failure doesn't corrupt output |

## Dependencies

No new production dependencies required.

**Dev dependencies to add**:
- `cargo-tarpaulin`: Coverage measurement
- `tempfile`: Already present for tests
- `proptest` (optional): Property-based testing for edge cases
