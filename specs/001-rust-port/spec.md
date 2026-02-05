# Feature Specification: Rust Port of Magento 2 Static Deploy

**Feature Branch**: `001-rust-port`
**Created**: 2025-02-05
**Status**: Draft
**Input**: Port magento2-static-deploy Go tool to Rust with same functionality and improved performance

## Clarifications

### Session 2025-02-05

- Q: Should performance have artificial limits? → A: No limits - maximize performance, saturate I/O bandwidth

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Deploy Hyva Theme Assets (Priority: P1)

As a Magento developer, I want to deploy static content for Hyva themes as fast as possible so that I can iterate quickly during development without waiting for the slow PHP-based deployment.

**Why this priority**: This is the primary use case - 90%+ of deployments are Hyva themes. The Go tool achieves 230-380x speedup here, and the Rust port must match or exceed this with no artificial performance limits.

**Independent Test**: Can be fully tested by running the tool against a Magento installation with a Hyva theme and measuring deployment time and file count.

**Acceptance Scenarios**:

1. **Given** a Magento 2 installation with a Hyva theme, **When** I run the deploy command with theme and locale parameters, **Then** all static assets are copied to `pub/static` as fast as I/O permits.

2. **Given** a Hyva theme with a parent theme chain (e.g., Custom → Hyva/reset → Magento/blank), **When** I deploy, **Then** files are copied in correct override order (child overrides parent).

3. **Given** module assets in `app/design/{area}/{Vendor}/{theme}/{Module_Name}/web/`, **When** I deploy, **Then** module overrides are correctly applied to the theme.

---

### User Story 2 - Parallel Multi-Theme Deployment (Priority: P2)

As a DevOps engineer, I want to deploy multiple themes and locales in parallel so that CI/CD pipelines complete faster.

**Why this priority**: Production sites often have multiple themes (frontend + adminhtml) and multiple locales. Parallel processing multiplies the time savings.

**Independent Test**: Can be tested by deploying 3+ themes with 2+ locales and verifying all combinations complete successfully with maximum CPU and I/O utilization.

**Acceptance Scenarios**:

1. **Given** multiple themes and locales specified, **When** I run deploy with `--jobs N`, **Then** deployment jobs run in parallel utilizing all specified workers to saturate available resources.

2. **Given** a deploy job fails for one theme, **When** other jobs are running, **Then** the tool reports the failure but continues processing remaining jobs.

3. **Given** verbose mode enabled, **When** deployment runs, **Then** I see progress output showing which theme/locale is being processed.

---

### User Story 3 - Luma Theme Fallback (Priority: P3)

As a developer maintaining legacy Luma themes, I want the tool to automatically delegate to Magento's native deployment for themes requiring LESS/RequireJS compilation.

**Why this priority**: While Hyva is the primary target, some projects still use Luma themes. The tool should gracefully handle this case rather than producing broken output.

**Independent Test**: Can be tested by deploying a Luma-based theme and verifying it delegates to `bin/magento` and produces correct CSS output.

**Acceptance Scenarios**:

1. **Given** a Luma-based theme (not Hyva), **When** I run deploy, **Then** the tool detects this and delegates to `bin/magento setup:static-content:deploy`.

2. **Given** a mixed deployment (Hyva + Luma themes), **When** I deploy, **Then** Hyva themes use fast file copying while Luma themes delegate to Magento.

---

### User Story 4 - Graceful Cancellation (Priority: P4)

As a developer, I want to be able to cancel a running deployment with Ctrl+C so that I can abort if I started with wrong parameters.

**Why this priority**: Long-running operations should be cancellable. The Go version lacks this, and it's a known improvement area.

**Independent Test**: Can be tested by starting a large deployment and pressing Ctrl+C, verifying the tool exits cleanly within 2 seconds.

**Acceptance Scenarios**:

1. **Given** a deployment is in progress, **When** I press Ctrl+C, **Then** the tool stops all workers and exits within 2 seconds.

2. **Given** cancellation occurs, **When** the tool exits, **Then** no corrupted partial files are left in `pub/static`.

---

### Edge Cases

- What happens when the Magento root path doesn't exist? Tool MUST exit with clear error message.
- What happens when `pub/static` is not writable? Tool MUST exit with permission error before starting work.
- How does the system handle symlinks in theme directories? Tool MUST follow symlinks and copy target content.
- What happens when a theme's parent doesn't exist? Tool MUST warn and continue with available parents.
- How does the system handle files with special characters in names? Tool MUST preserve exact filenames.
- What happens when disk runs out of space mid-deployment? Tool MUST report error and exit cleanly.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Tool MUST accept Magento root path as positional argument or default to current directory.
- **FR-002**: Tool MUST accept `--area` flag with values: `frontend`, `adminhtml`, or both (default: both).
- **FR-003**: Tool MUST accept `--theme` flag to specify one or more themes in `Vendor/name` format.
- **FR-004**: Tool MUST accept `--locale` flag to specify one or more locales (default: `en_US`).
- **FR-005**: Tool MUST accept `--jobs` flag to control parallelism (default: number of CPU cores).
- **FR-006**: Tool MUST accept `--verbose` flag for detailed progress output.
- **FR-007**: Tool MUST discover all themes if `--theme` not specified.
- **FR-008**: Tool MUST resolve theme parent chains from `theme.xml` files.
- **FR-009**: Tool MUST detect Hyva vs Luma themes by checking for `Hyva_Theme` dependency or Hyva parent.
- **FR-010**: Tool MUST copy files from theme web directories preserving directory structure.
- **FR-011**: Tool MUST copy files from `lib/web/` for library assets.
- **FR-012**: Tool MUST copy module assets from vendor directories.
- **FR-013**: Tool MUST handle theme module overrides in `app/design/{area}/{Vendor}/{theme}/{Module}/web/`.
- **FR-014**: Tool MUST create version-specific directories under `pub/static`.
- **FR-015**: Tool MUST read deployed version from `pub/static/deployed_version.txt`.
- **FR-016**: Tool MUST output summary with file count and elapsed time on completion.
- **FR-017**: Tool MUST return exit code 0 on success, non-zero on failure.
- **FR-018**: Tool MUST handle SIGINT/SIGTERM for graceful shutdown.

### Performance Requirements

- **PR-001**: Tool MUST NOT impose artificial limits on throughput, memory, or parallelism.
- **PR-002**: Tool MUST saturate available I/O bandwidth when processing files.
- **PR-003**: Tool MUST utilize all available CPU cores when `--jobs` equals or exceeds core count.
- **PR-004**: Tool MUST use zero-copy file operations where supported by the operating system.
- **PR-005**: Tool MUST minimize memory allocations in hot paths (file copying loops).
- **PR-006**: Tool MUST pre-allocate buffers and reuse them across operations.
- **PR-007**: Tool MUST use memory-mapped I/O for large files when beneficial.
- **PR-008**: Tool SHOULD exceed Go implementation performance (baseline: 40,000+ files/second).

### Key Entities

- **Theme**: Represents a Magento theme with vendor, name, area, parent chain, and type (Hyva/Luma).
- **DeployJob**: A unit of work combining theme, locale, and area for parallel processing.
- **DeployResult**: Outcome of a job including success/failure, file count, duration, and any errors.
- **FileSource**: Origin of static files (theme web, lib, vendor module, theme module override).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Tool MUST match or exceed Go tool throughput (baseline: 40,000 files/second) with no artificial ceiling.
- **SC-002**: Tool MUST saturate disk I/O bandwidth on NVMe/SSD storage during file operations.
- **SC-003**: Tool MUST scale linearly with additional CPU cores up to I/O saturation.
- **SC-004**: All files deployed by the Go tool are also deployed by the Rust tool (feature parity).
- **SC-005**: Cancellation via Ctrl+C completes within 2 seconds.
- **SC-006**: Error messages clearly indicate the problem and affected file/theme.
- **SC-007**: Tool works on Linux and macOS without modification.

## Assumptions

- Magento 2.4+ installation structure is expected.
- PHP is available on PATH for Luma theme delegation.
- File system supports the required concurrent file operations.
- User running the tool has read access to Magento files and write access to `pub/static`.
- No artificial performance limits are desired - tool should maximize throughput.
