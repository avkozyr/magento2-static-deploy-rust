# Tasks: Rust Performance Optimizations

**Input**: Design documents from `/specs/002-rust-optimizations/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md

**Tests**: Not creating test tasks per constitution ("MUST NOT create tests unless explicitly requested"). Coverage target is aspirational.

**Organization**: Tasks are grouped by user story to enable independent implementation and benchmarking of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Baseline Measurement)

**Purpose**: Establish performance baseline before any optimizations

- [ ] T001 Run `cargo bench -- --save-baseline before` to capture baseline metrics in benches/deploy_benchmark.rs
- [ ] T002 Document current throughput (files/sec) and memory usage as baseline reference
- [ ] T003 [P] Verify `cargo clippy -- -D warnings` passes with no warnings

**Checkpoint**: Baseline established - optimization work can begin

---

## Phase 2: User Story 1 - Faster Deployments (Priority: P1) 🎯 MVP

**Goal**: Achieve 25%+ performance improvement through heap allocation reduction, pre-allocation, and I/O optimization

**Independent Test**: Run `cargo bench -- --baseline before` and verify throughput improvement ≥25%

### High-Impact Optimizations (FR-001, FR-005, FR-006)

- [ ] T004 [P] [US1] Replace `format!()` with `Path::join()` for path construction in src/copier.rs (FR-001)
- [ ] T005 [P] [US1] Replace `Arc<T>` with `&T` references where ownership not required in src/scanner.rs (FR-005)
- [ ] T006 [P] [US1] Add `Vec::with_capacity()` pre-allocation in file collection loops in src/copier.rs (FR-006)
- [ ] T007 [P] [US1] Add `Vec::with_capacity()` pre-allocation in theme discovery in src/scanner.rs (FR-006)

### Medium-Impact Optimizations (FR-002, FR-003, FR-004, FR-009, FR-011)

- [ ] T008 [P] [US1] Add `#[inline]` hints to accessor methods in src/theme.rs (FR-002)
- [ ] T009 [P] [US1] Use zero-copy XML parsing with borrowed slices in src/scanner.rs (FR-003)
- [ ] T010 [P] [US1] Configure Rayon thread pool for I/O-bound workloads in src/deployer.rs (FR-004)
- [ ] T011 [P] [US1] Implement progress bar batching (every 100 files) in src/deployer.rs (FR-009)
- [ ] T012 [P] [US1] Increase file copy buffer size to 64KB in src/copier.rs (FR-011)

### Validation

- [ ] T013 [US1] Run `cargo bench -- --baseline before` and verify ≥25% improvement
- [ ] T014 [US1] Verify no new Clippy warnings with `cargo clippy -- -D warnings`

**Checkpoint**: User Story 1 complete - performance target achieved

---

## Phase 3: User Story 2 - Better Error Diagnostics (Priority: P2)

**Goal**: Include source and destination paths in all error messages for faster debugging

**Independent Test**: Trigger file copy error and verify error message contains both paths

### Implementation

- [ ] T015 [P] [US2] Add source/destination path context to file copy errors in src/copier.rs (FR-008)
- [ ] T016 [P] [US2] Add operation context to directory creation errors in src/copier.rs (FR-008)
- [ ] T017 [P] [US2] Add path context to theme discovery errors in src/scanner.rs (FR-008)
- [ ] T018 [US2] Implement LocaleCode validation with clear error message in src/theme.rs (FR-010)
- [ ] T019 [US2] Update main.rs to validate locale codes at CLI boundary using LocaleCode::new()

### Validation

- [ ] T020 [US2] Verify error messages include paths by triggering permission error manually
- [ ] T021 [US2] Verify invalid locale codes (e.g., "invalid") produce clear error message

**Checkpoint**: User Story 2 complete - error diagnostics improved

---

## Phase 4: User Story 3 - Improved Code Quality (Priority: P3)

**Goal**: Add derived traits and documentation comments for maintainability

**Independent Test**: Run `cargo doc --no-deps` and verify all public items have documentation

### Trait Derivations (FR-007)

- [ ] T022 [P] [US3] Add `#[derive(Eq, PartialEq, Hash)]` to ThemeCode in src/theme.rs
- [ ] T023 [P] [US3] Add `#[derive(Eq, PartialEq, Hash)]` to LocaleCode in src/theme.rs
- [ ] T024 [P] [US3] Add `#[derive(Eq, PartialEq)]` to Area enum in src/theme.rs

### Documentation Comments (FR-012)

- [ ] T025 [P] [US3] Add doc comments to all public types/functions in src/theme.rs
- [ ] T026 [P] [US3] Add doc comments to all public types/functions in src/scanner.rs
- [ ] T027 [P] [US3] Add doc comments to all public types/functions in src/copier.rs
- [ ] T028 [P] [US3] Add doc comments to all public types/functions in src/deployer.rs
- [ ] T029 [P] [US3] Add doc comments to all public types/functions in src/error.rs
- [ ] T030 [P] [US3] Add doc comments to all public types/functions in src/config.rs
- [ ] T031 [P] [US3] Add module-level doc comments to src/lib.rs

### Validation

- [ ] T032 [US3] Run `cargo doc --no-deps` and verify no missing documentation warnings
- [ ] T033 [US3] Verify Clippy passes with `cargo clippy -- -D warnings`

**Checkpoint**: User Story 3 complete - code quality improved

---

## Phase 5: Polish & Final Validation

**Purpose**: Final benchmarks and cleanup

- [ ] T034 Run final `cargo bench -- --baseline before` and document improvement percentage
- [ ] T035 Update README.md benchmark section with new performance numbers
- [ ] T036 Run `cargo fmt` to ensure consistent formatting
- [ ] T037 Verify all success criteria from spec.md are met

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies - establish baseline first
- **Phase 2 (US1)**: Depends on Phase 1 baseline - core performance work
- **Phase 3 (US2)**: Can run in parallel with Phase 2 (different files mostly)
- **Phase 4 (US3)**: Can run in parallel with Phase 2/3 (additive changes)
- **Phase 5 (Polish)**: Depends on all user stories being complete

### User Story Independence

- **US1**: Performance optimizations - independent, no cross-story dependencies
- **US2**: Error diagnostics - independent, LocaleCode validation can start anytime
- **US3**: Documentation - fully independent, can run in parallel with all stories

### Parallel Opportunities

**Within Phase 2 (US1)**: All T004-T012 can run in parallel (different files/concerns)

**Across Stories**: US1, US2, US3 can all be worked on in parallel after baseline:
- US1 touches: copier.rs (core), scanner.rs (XML), deployer.rs (threading)
- US2 touches: copier.rs (errors), scanner.rs (errors), theme.rs (validation)
- US3 touches: all files (additive doc comments only)

---

## Parallel Example: User Story 1

```bash
# Launch all high-impact optimizations together:
Task: "T004 Replace format!() with Path::join() in src/copier.rs"
Task: "T005 Replace Arc<T> with &T references in src/scanner.rs"
Task: "T006 Add Vec::with_capacity() in src/copier.rs"
Task: "T007 Add Vec::with_capacity() in src/scanner.rs"

# Then launch medium-impact optimizations:
Task: "T008 Add #[inline] hints in src/theme.rs"
Task: "T009 Zero-copy XML parsing in src/scanner.rs"
Task: "T010 Configure Rayon thread pool in src/deployer.rs"
Task: "T011 Progress bar batching in src/deployer.rs"
Task: "T012 64KB buffer size in src/copier.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Establish baseline
2. Complete Phase 2: US1 performance optimizations
3. **STOP and VALIDATE**: Run `cargo bench` to verify 25% improvement
4. If target met: Continue to US2/US3
5. If target not met: Profile and identify remaining bottlenecks

### Incremental Delivery

1. Baseline → Validate measurement capability
2. Add US1 → Benchmark → 25% faster (MVP!)
3. Add US2 → Test errors → Better diagnostics
4. Add US3 → Doc generation → Maintainable codebase
5. Each story adds value without breaking previous stories

---

## Notes

- All tasks preserve backwards compatibility (Out of Scope constraint)
- No architectural changes required (existing Rayon pattern preserved)
- Benchmark before/after every optimization to catch regressions
- [P] tasks = different files or purely additive changes
- Commit after each task with benchmark results for perf changes
