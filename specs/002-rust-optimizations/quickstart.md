# Quickstart: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2026-02-05

## Prerequisites

- Rust 1.75+ (`rustup update stable`)
- Criterion for benchmarks (`cargo install cargo-criterion`)
- A Magento 2 installation for integration testing

## Development Setup

```bash
# Clone and enter project
cd /path/to/magento2-static-deploy-rust

# Checkout feature branch
git checkout 002-rust-optimizations

# Build in release mode
cargo build --release

# Run tests
cargo test

# Run clippy (must pass with no warnings)
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Benchmarking Workflow

### 1. Establish Baseline (Before Changes)

```bash
# Run and save baseline
cargo bench -- --save-baseline before

# Results saved to target/criterion/
```

### 2. Make Optimization Changes

Apply changes from the task list, one at a time.

### 3. Compare Against Baseline

```bash
# Run comparison
cargo bench -- --baseline before

# Look for improvements (negative % = faster)
```

### 4. Accept or Reject

- If regression > 5%: investigate and fix
- If improvement: document in commit message
- If neutral: acceptable if code quality improves

## Key Files to Modify

| File | Optimizations |
|------|---------------|
| `src/theme.rs` | FR-002 (inline), FR-007 (traits), FR-010 (validation) |
| `src/copier.rs` | FR-001 (paths), FR-006 (pre-alloc), FR-008 (errors), FR-011 (buffer) |
| `src/scanner.rs` | FR-003 (zero-copy XML), FR-005 (borrowing) |
| `src/deployer.rs` | FR-004 (thread pool), FR-009 (progress batch) |
| `src/*.rs` | FR-012 (documentation) |

## Testing Commands

```bash
# Unit tests
cargo test

# Single benchmark
cargo bench --bench deploy_benchmark

# With flamegraph (requires cargo-flamegraph)
cargo flamegraph --bench deploy_benchmark

# Memory profiling (requires heaptrack)
heaptrack target/release/magento-static-deploy /path/to/magento
```

## Validation Checklist

Before marking any task complete:

- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo test` passes
- [ ] `cargo bench` shows no regression > 5%
- [ ] Commit message includes benchmark results (for perf changes)

## Common Issues

### "unwrap_used" lint error
Use `.context("description")?` instead of `.unwrap()`

### Benchmark variance too high
Ensure no other CPU-intensive processes running. Use `--warm-up-time 5` for more stable results.

### Memory usage not decreasing
Use `heaptrack` to identify allocation sites. Check for retained Vecs or Strings.
