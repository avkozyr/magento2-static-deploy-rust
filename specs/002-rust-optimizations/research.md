# Research: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2026-02-05

## Overview

Research findings for 12 high-impact Rust optimizations targeting the Magento 2 static deploy tool.

---

## 1. Path Joining vs String Formatting (FR-001)

**Decision**: Use `Path::join()` instead of `format!()` for path construction

**Rationale**:
- `format!()` allocates a new String on every call
- `Path::join()` returns PathBuf with platform-aware separators
- In hot loops, avoiding format allocations reduces heap pressure significantly

**Alternatives Considered**:
- `format!()` - Rejected: allocates on every call
- `concat!()` - Not applicable: compile-time only, requires literals

---

## 2. Inline Hints for Hot Methods (FR-002)

**Decision**: Add `#[inline]` to frequently-called accessor methods

**Rationale**:
- Small methods benefit from inlining to avoid function call overhead
- Compiler may not inline across crate boundaries without hints
- Accessors like `as_str()`, `vendor()`, `name()` are called in hot loops

**Alternatives Considered**:
- `#[inline(always)]` - Rejected: too aggressive, can bloat binary
- No annotation - Current: relies on compiler heuristics

---

## 3. Zero-Copy XML Parsing (FR-003)

**Decision**: Use quick-xml's zero-copy reader with borrowed slices

**Rationale**:
- quick-xml supports `&[u8]` slices without allocation
- Theme XML files are small (<1KB typically)
- Memory-mapping + zero-copy parsing eliminates all read allocations

**Alternatives Considered**:
- `read_to_string()` + parse - Rejected: allocates string buffer
- serde_xml - Rejected: requires owned types, more allocations

---

## 4. Thread Pool Configuration (FR-004)

**Decision**: Configure Rayon thread pool size based on workload characteristics

**Rationale**:
- Default Rayon uses num_cpus which may not be optimal for I/O-bound work
- File copying is I/O-bound; more threads can hide latency
- Configurable via `RAYON_NUM_THREADS` or explicit pool

**Alternatives Considered**:
- Fixed thread count - Rejected: not portable across machines
- Async I/O (Tokio) - Rejected: adds complexity, Rayon sufficient for file copying

---

## 5. Borrowing Over Arc (FR-005)

**Decision**: Use `&T` references instead of `Arc<T>` when ownership not required

**Rationale**:
- `Arc::clone()` involves atomic increment (memory barrier)
- References have zero runtime cost
- Theme data is read-only during deployment

**Alternatives Considered**:
- `Rc` - Rejected: not thread-safe for Rayon
- `Arc` everywhere - Current: unnecessary overhead for read-only data

---

## 6. Pre-allocation (FR-006)

**Decision**: Use `Vec::with_capacity()` when size is known or estimable

**Rationale**:
- Growing Vec doubles capacity, causing reallocation + copy
- Theme discovery knows approximate file count from directory traversal
- Pre-allocation eliminates intermediate allocations

**Alternatives Considered**:
- Default Vec::new() - Current: causes multiple reallocations
- Fixed-size arrays - Not applicable: size unknown at compile time

---

## 7. Derive Eq/Hash for Core Types (FR-007)

**Decision**: Add `#[derive(Eq, PartialEq, Hash)]` to ThemeCode, LocaleCode

**Rationale**:
- Enables use in HashSet/HashMap for deduplication
- Enables equality comparison in tests
- Zero runtime cost (compiler-generated)

**Alternatives Considered**:
- Manual impl - Rejected: error-prone, no benefit
- No Eq - Current: limits usability

---

## 8. Error Context Enhancement (FR-008)

**Decision**: Include source path, destination path, and operation in all errors

**Rationale**:
- Failed file copies are common (permissions, disk full)
- Without paths, debugging requires manual investigation
- anyhow's `.context()` adds zero-cost context chain

**Alternatives Considered**:
- Log-only - Rejected: errors may not reach logs
- Custom error types - More work, anyhow sufficient

---

## 9. Progress Bar Batching (FR-009)

**Decision**: Batch progress updates to reduce atomic operation overhead

**Rationale**:
- indicatif uses AtomicU64 internally
- Updating per-file causes cache line contention
- Batching (e.g., every 100 files) reduces overhead significantly

**Alternatives Considered**:
- Per-file updates - Current: high contention
- No progress - Rejected: poor UX for long operations

---

## 10. Locale Code Validation (FR-010)

**Decision**: Validate locale format (xx_YY) at input boundary

**Rationale**:
- Invalid locales cause silent failures or wrong directory creation
- Early validation provides clear error messages
- Regex or manual parsing both efficient for simple pattern

**Alternatives Considered**:
- No validation - Current: fails silently
- Full i18n library - Overkill for format check

---

## 11. Buffer Size Optimization (FR-011)

**Decision**: Use 64KB buffer for file copying (optimal for modern NVMe)

**Rationale**:
- Default std::fs::copy uses 8KB buffer
- Modern NVMe SSDs have 4KB pages, benefit from larger transfers
- 64KB balances memory use vs I/O efficiency

**Alternatives Considered**:
- 8KB (default) - Current: suboptimal for NVMe
- 1MB - Excessive memory use with many parallel copies
- Memory-mapped copy - Complex, minimal benefit for typical file sizes

---

## 12. Documentation Comments (FR-012)

**Decision**: Add `///` doc comments to all public types and functions

**Rationale**:
- Enables `cargo doc` generation
- IDE support for inline documentation
- Required for library consumers

**Alternatives Considered**:
- README only - Rejected: not accessible from code
- `//` comments - Rejected: not extracted by rustdoc

---

## Summary

All 12 optimizations researched with clear decisions and implementation patterns. No NEEDS CLARIFICATION items remaining.

| Optimization | Impact | Complexity | Priority |
|--------------|--------|------------|----------|
| Path joining (FR-001) | High | Low | P1 |
| Inline hints (FR-002) | Medium | Low | P2 |
| Zero-copy XML (FR-003) | Medium | Medium | P2 |
| Thread pool config (FR-004) | Medium | Low | P2 |
| Borrowing over Arc (FR-005) | High | Medium | P1 |
| Pre-allocation (FR-006) | High | Low | P1 |
| Derive Eq/Hash (FR-007) | Low | Low | P3 |
| Error context (FR-008) | Medium | Low | P2 |
| Progress batching (FR-009) | Medium | Low | P2 |
| Locale validation (FR-010) | Low | Low | P3 |
| Buffer size (FR-011) | Medium | Low | P2 |
| Documentation (FR-012) | Low | Medium | P3 |
