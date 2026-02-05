# Data Model: Rust Port of Magento 2 Static Deploy

**Feature**: 001-rust-port
**Date**: 2025-02-05

## Overview

This document defines the core data structures for the Magento 2 static content deployment tool. All types follow Rust conventions and constitution principles (borrowing over ownership, no allocations in hot paths).

---

## Core Entities

### 1. Theme

Represents a Magento theme with its configuration and parent chain.

```rust
/// A Magento theme with its metadata and inheritance chain
#[derive(Debug, Clone)]
pub struct Theme {
    /// Vendor name (e.g., "Hyva", "Magento")
    pub vendor: String,

    /// Theme name (e.g., "reset", "luma")
    pub name: String,

    /// Area: "frontend" or "adminhtml"
    pub area: Area,

    /// Full path to theme directory
    pub path: PathBuf,

    /// Parent theme in inheritance chain (None if root)
    pub parent: Option<String>,

    /// Theme type determines deployment strategy
    pub theme_type: ThemeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Frontend,
    Adminhtml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeType {
    /// Hyva theme: fast file copy, no LESS compilation
    Hyva,
    /// Luma theme: delegate to bin/magento for LESS/RequireJS
    Luma,
}
```

**Relationships**:
- Theme → parent Theme (0..1) via `parent` field
- Theme → Area (1) via `area` field
- Theme → ThemeType (1) via `theme_type` field

**Validation Rules**:
- `vendor` and `name` must be non-empty
- `path` must exist and be readable
- `area` must be valid enum variant

---

### 2. DeployJob

A unit of work for parallel processing. Immutable once created.

```rust
/// A deployment job combining theme, locale, and area
#[derive(Debug, Clone)]
pub struct DeployJob {
    /// Theme to deploy
    pub theme: Theme,

    /// Locale code (e.g., "en_US", "nl_NL")
    pub locale: String,

    /// Resolved parent chain (ordered: child first, root last)
    pub parent_chain: Vec<Theme>,
}
```

**Relationships**:
- DeployJob → Theme (1) primary theme
- DeployJob → Theme (0..N) parent chain
- DeployJob → Locale (1) via `locale` field

**Validation Rules**:
- `locale` must match pattern `[a-z]{2}_[A-Z]{2}`
- `parent_chain` must be ordered child-first

---

### 3. DeployResult

Outcome of a deployment job. Used for reporting and error aggregation.

```rust
/// Result of a deployment job
#[derive(Debug)]
pub struct DeployResult {
    /// Which job this result belongs to
    pub job: DeployJob,

    /// Success or failure status
    pub status: DeployStatus,

    /// Number of files copied
    pub file_count: u64,

    /// Time taken for this job
    pub duration: Duration,
}

#[derive(Debug)]
pub enum DeployStatus {
    /// Successfully deployed
    Success,

    /// Failed with error
    Failed(DeployError),

    /// Cancelled by user (SIGINT)
    Cancelled,

    /// Delegated to bin/magento (Luma themes)
    Delegated,
}
```

**Relationships**:
- DeployResult → DeployJob (1)
- DeployResult → DeployStatus (1)
- DeployStatus::Failed → DeployError (1)

---

### 4. FileSource

Represents the origin of static files to copy.

```rust
/// Origin of static files for a theme
#[derive(Debug, Clone)]
pub enum FileSource {
    /// Theme's own web directory: app/design/{area}/{Vendor}/{theme}/web/
    ThemeWeb {
        theme: String,
        path: PathBuf,
    },

    /// Library files: lib/web/
    Library {
        path: PathBuf,
    },

    /// Vendor module assets: vendor/{vendor}/{module}/view/{area}/web/
    VendorModule {
        module: String,
        path: PathBuf,
    },

    /// Theme module override: app/design/{area}/{Vendor}/{theme}/{Module}/web/
    ThemeModuleOverride {
        theme: String,
        module: String,
        path: PathBuf,
    },
}
```

**Usage**: File sources are processed in priority order (theme overrides > module defaults).

---

### 5. Config

Runtime configuration from CLI arguments.

```rust
/// CLI configuration parsed from arguments
#[derive(Debug, Clone)]
pub struct Config {
    /// Magento root directory
    pub magento_root: PathBuf,

    /// Areas to deploy
    pub areas: Vec<Area>,

    /// Themes to deploy (None = all discovered)
    pub themes: Option<Vec<String>>,

    /// Locales to deploy
    pub locales: Vec<String>,

    /// Number of parallel workers
    pub jobs: usize,

    /// Enable verbose output
    pub verbose: bool,
}
```

**Defaults**:
- `magento_root`: current directory
- `areas`: `[Frontend, Adminhtml]`
- `themes`: None (discover all)
- `locales`: `["en_US"]`
- `jobs`: number of CPU cores
- `verbose`: false

---

## State Transitions

### DeployJob Lifecycle

```
Created → Queued → Processing → Completed
                       │
                       ├── Success
                       ├── Failed
                       ├── Cancelled
                       └── Delegated
```

1. **Created**: Job constructed from Config + Theme
2. **Queued**: Added to Rayon work queue
3. **Processing**: Worker thread executing
4. **Completed**: Result recorded with status

---

## File Copying Data Flow

```
FileSource[] → filter (exists) → sort (priority) → copy (parallel) → Result
```

Priority order (highest to lowest):
1. ThemeModuleOverride (theme-specific module customization)
2. ThemeWeb (theme's own assets)
3. VendorModule (module defaults)
4. Library (lib/web shared assets)

---

## Atomic Counters (Shared State)

```rust
/// Global counters for progress tracking (per constitution: atomics only)
pub struct DeployStats {
    pub files_copied: AtomicU64,
    pub bytes_copied: AtomicU64,
    pub errors: AtomicU64,
}
```

**Thread Safety**: All counters use `Ordering::Relaxed` for maximum performance (exact counts not critical during processing, only at completion).

---

## Path Conventions

| Entity | Path Pattern |
|--------|--------------|
| Theme | `app/design/{area}/{Vendor}/{name}/` |
| Theme XML | `app/design/{area}/{Vendor}/{name}/theme.xml` |
| Theme Web | `app/design/{area}/{Vendor}/{name}/web/` |
| Library | `lib/web/` |
| Vendor Module | `vendor/{vendor}/{module}/view/{area}/web/` |
| Output | `pub/static/{area}/{Vendor}/{name}/{locale}/` |
| Version | `pub/static/deployed_version.txt` |
