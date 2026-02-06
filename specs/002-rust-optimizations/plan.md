# Implementation Plan: Rust Performance Optimizations

**Branch**: `002-rust-optimizations` | **Date**: 2026-02-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-rust-optimizations/spec.md`

## Summary

Apply 12 high-impact Rust optimizations to the Magento 2 static deploy tool to achieve 25%+ performance improvement. Focus areas: minimize heap allocations, add inline hints, improve error context, batch progress updates, pre-allocate collections, and add comprehensive documentation.

## Technical Context

**Language/Version**: Rust 1.75+ (MSRV from Cargo.toml)
**Primary Dependencies**: rayon 1.10, quick-xml 0.31, memmap2 0.9, walkdir 2, anyhow 1, thiserror 2, clap 4, indicatif 0.17
**Storage**: File system (pub/static/ directory)
**Testing**: cargo test (unit), Criterion benchmarks (performance)
**Target Platform**: Linux x86_64, Linux ARM64, macOS ARM64
**Project Type**: Single binary with library
**Performance Goals**: 50k+ ops/sec throughput, <5ms p99 latency, <256MB memory peak
**Constraints**: No architectural changes, preserve backwards compatibility
**Scale/Scope**: 10,000-100,000 static files per deployment

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| I. Performance First | PASS | All 12 optimizations target throughput/memory/latency |
| II. Memory Safety | PASS | Plan uses `.context()`, avoids `.unwrap()`, pre-allocates |
| III. Concurrency Excellence | PASS | Preserves Rayon, uses atomics for progress |
| IV. Zero-Copy Optimization | PASS | FR-003 zero-copy XML, FR-005 borrowing over Arc |
| V. Benchmarking Discipline | PASS | SC-001 requires Criterion baseline/comparison |

**Gate Result**: PASS - All principles satisfied. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/002-rust-optimizations/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # N/A (no API changes)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Library entry, re-exports
├── main.rs              # CLI binary
├── config.rs            # CLI configuration
├── theme.rs             # Theme/locale types (ThemeCode, LocaleCode)
├── scanner.rs           # Theme discovery, XML parsing
├── deployer.rs          # Deployment orchestration
├── copier.rs            # File copying logic
└── error.rs             # Error types

benches/
└── deploy_benchmark.rs  # Criterion benchmarks
```

**Structure Decision**: Single project with library and binary. No structural changes needed - optimizations apply to existing modules.

## Complexity Tracking

> No Constitution violations. All changes are within existing architecture.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| N/A | N/A | N/A |
