<!--
SYNC IMPACT REPORT
==================
Version change: 1.0.0 → 1.1.0 (MINOR)
Modified principles:
  - Removed "MUST NOT create tests" from Development Workflow
Added sections:
  - Testing Standards (80% coverage requirement)
Removed sections: N/A
Templates requiring updates:
  - .specify/templates/plan-template.md ✅ (no changes needed)
  - .specify/templates/spec-template.md ✅ (no changes needed)
  - .specify/templates/tasks-template.md ✅ (no changes needed)
Follow-up TODOs: Add tests to reach 80% coverage target
-->

# Magento 2 Static Deploy Rust Constitution

High-performance static content deployment tool for Magento 2, written in Rust.
Target: Match or exceed 230-380x speedup over PHP baseline achieved by Go implementation.

## Core Principles

### I. Performance First

All code MUST prioritize performance. Every design decision MUST consider throughput,
latency, memory usage, and CPU utilization impact.

**Non-negotiables**:
- Throughput target: 50k+ operations/second
- Latency p99: < 5ms per file operation
- Memory peak: < 256MB regardless of dataset size
- CPU utilization: 90%+ on available cores

**Rationale**: This tool exists solely to provide extreme performance gains over PHP.
Performance regressions are defects.

### II. Memory Safety Without Compromise

All code MUST follow Rust's ownership model correctly. Production code MUST NOT panic.

**Non-negotiables**:
- MUST propagate errors with context using `?` and `.context()`
- MUST NOT use `.unwrap()` or `.expect()` in production code paths
- MUST pre-allocate collections with `with_capacity()` when size is known
- MUST prefer borrowing (`&T`, `&mut T`) over ownership transfer
- MUST document all `unsafe` blocks with `// SAFETY:` comments explaining invariants

**Rationale**: Panics in production break deployments. Memory safety is Rust's core value.

### III. Concurrency Excellence

All concurrent code MUST use appropriate patterns for the workload type.

**Non-negotiables**:
- MUST use Rayon for CPU-bound parallel work (file processing, transformations)
- MUST use Tokio for I/O-bound async work (if network operations needed)
- MUST use atomics (`AtomicU64`, etc.) for progress tracking, not mutexes
- MUST NOT use `Arc<Mutex<T>>` when channels or atomics suffice
- MUST NOT block async runtime with synchronous operations

**Rationale**: Wrong concurrency patterns cause deadlocks, poor utilization, or hidden bottlenecks.

### IV. Zero-Copy Optimization

Hot paths MUST minimize allocations and copies.

**Non-negotiables**:
- MUST use `&str` over `String`, `&[T]` over `Vec<T>` when ownership not required
- MUST use memory-mapped files (`memmap2`) for large file reads
- MUST pool and reuse buffers in hot loops
- MUST NOT allocate inside hot loops (no `format!()`, `String::new()`, `vec![]`)
- SHOULD use `SmallVec<[T; N]>` for small, stack-allocatable collections
- SHOULD intern repeated strings with `Arc<str>` + `DashMap`

**Rationale**: Allocations are the primary performance killer in high-throughput file processing.

### V. Benchmarking Discipline

All performance-critical changes MUST be measured before and after.

**Non-negotiables**:
- MUST run `cargo bench` baseline before optimization work
- MUST run `cargo bench` comparison after changes
- MUST justify any regression > 5% or reject the change
- MUST include benchmark results in commit messages for performance changes
- SHOULD profile with flamegraph before and after optimization

**Rationale**: Unmeasured optimizations are guesses. Regressions compound silently.

## Performance Standards

| Metric | Target | Tool |
|--------|--------|------|
| Throughput | 50k ops/sec | `cargo bench` |
| Latency p99 | < 5ms | Histogram metrics |
| Memory peak | < 256MB | `heaptrack` |
| CPU utilization | 90%+ | `perf stat` |

Build profile for release:
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

## Development Workflow

- MUST create a plan before code changes and get approval before implementation
- MUST keep plans minimal; avoid complex changes unless explicitly requested
- MUST work task-by-task with proper status updates
- MUST run `cargo clippy -- -D warnings` and `cargo fmt` after each change
- MUST profile before and after optimization work
- MUST document performance tradeoffs in commit messages
- MUST ask for clarification before implementing unclear requirements

## Testing Standards

- MUST maintain minimum 80% test coverage across all modules
- MUST write unit tests for all public functions and types
- MUST test error paths and edge cases, not just happy paths
- MUST run `cargo tarpaulin` to verify coverage before merging
- SHOULD use property-based testing for parsing and validation logic

## Governance

This constitution is the authoritative source for project standards. All code reviews
MUST verify compliance with these principles.

**Amendment Process**:
1. Propose change with rationale
2. Document impact on existing code
3. Update version according to semver:
   - MAJOR: Principle removal or incompatible redefinition
   - MINOR: New principle or material expansion
   - PATCH: Clarification or wording fix
4. Update dependent templates if principle names change

**Compliance**:
- All PRs MUST pass Constitution Check in plan-template.md
- Violations MUST be justified in Complexity Tracking table
- See `CLAUDE.md` for runtime development guidance

**Version**: 1.1.0 | **Ratified**: 2025-02-05 | **Last Amended**: 2026-02-06
