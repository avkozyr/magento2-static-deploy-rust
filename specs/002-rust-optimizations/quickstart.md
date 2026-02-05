# Quickstart: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2025-02-05

## Prerequisites

- Rust 1.75+ installed
- cargo-tarpaulin for coverage: `cargo install cargo-tarpaulin`
- Access to Magento 2 installation for validation

## Development Workflow

### 1. Establish Baseline

Before making any changes, capture the current performance:

```bash
# Build release binary
cargo build --release

# Run benchmarks
cargo bench -- --save-baseline before

# Record throughput on real Magento
./target/release/magento-static-deploy /path/to/magento \
  --theme Vendor/theme --locale en_US -v
```

### 2. Run Tests

```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin --out Html
open tarpaulin-report.html
```

### 3. Make Changes

Implement optimizations in order:

1. **Phase 1 (P1)**: Performance optimizations
2. **Phase 2 (P2)**: API improvements
3. **Phase 3 (P3)**: Test coverage

### 4. Validate Changes

After each optimization:

```bash
# Check compilation
cargo check

# Run Clippy
cargo clippy -- -D warnings

# Run tests
cargo test

# Compare benchmarks
cargo bench -- --baseline before

# Check coverage
cargo tarpaulin --out Html
```

### 5. Final Validation

```bash
# Build optimized release
cargo build --release

# Run against real Magento
./target/release/magento-static-deploy /path/to/magento \
  --theme Vendor/theme --locale en_US,nl_NL -v

# Verify 25% improvement
# Expected: 13,500+ files/sec (vs ~10,800 baseline)
```

## File-by-File Changes

### theme.rs
- Add `#[inline]` to accessor methods
- Add `PartialEq`, `Eq`, `Hash` derives to ThemeCode, LocaleCode
- Add doc comments to all public items

### scanner.rs
- Add `Vec::with_capacity()` for source collections
- Improve module.xml parsing efficiency

### deployer.rs
- Replace `format!()` with `PathBuf::join()`
- Pass `&Arc<Theme>` instead of cloning
- Implement thread-local progress batching

### copier.rs
- Increase BUFFER_SIZE to 256KB
- Add source/dest to error context

### main.rs
- Configure Rayon thread pool for I/O (2× cores)

### config.rs
- Add locale format validation

### error.rs
- Update CopyFailed to include both paths

## Test File Locations

```text
tests/
├── unit/
│   ├── theme_test.rs
│   ├── scanner_test.rs
│   ├── deployer_test.rs
│   ├── copier_test.rs
│   └── config_test.rs
└── integration/
    └── deploy_test.rs
```

## Coverage Targets

| Module | Target | Key Scenarios |
|--------|--------|---------------|
| theme.rs | 100% | Parse valid/invalid, compare, hash |
| scanner.rs | 100% | Discover themes, scan sources |
| deployer.rs | 100% | Job matrix, deploy flow |
| copier.rs | 100% | Copy success, failure, cancel |
| config.rs | 100% | Valid/invalid CLI args |
| error.rs | 100% | All error Display formats |

## Troubleshooting

### Coverage Not Reaching 100%

Check for:
- Unreachable error branches (may need mock injection)
- Platform-specific code paths
- Feature-gated code

### Benchmark Regression

If benchmarks show regression:
1. Check if optimization actually reduces allocations
2. Profile with `flamegraph` to find bottleneck
3. Consider if trade-off is acceptable

### Test Failures

Common issues:
- Temp directory permissions
- Symlink handling on different platforms
- Timing-sensitive tests in parallel execution
