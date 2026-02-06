# Data Model: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2026-02-05

## Overview

Core entities for the Magento 2 static deploy tool. This feature enhances existing types with traits and validation.

---

## Entities

### ThemeCode

Represents a Magento theme identifier in "Vendor/name" format.

| Field | Type | Description |
|-------|------|-------------|
| inner | `String` | Full theme code (e.g., "Hyva/default") |

**Derived Traits** (FR-007):
- `Debug`, `Clone`, `Eq`, `PartialEq`, `Hash`

**Methods**:
| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `fn new(code: &str) -> Result<Self>` | Validates and creates ThemeCode |
| `vendor` | `fn vendor(&self) -> &str` | Returns vendor part (before `/`) |
| `name` | `fn name(&self) -> &str` | Returns theme name (after `/`) |
| `as_str` | `#[inline] fn as_str(&self) -> &str` | Returns full code (FR-002) |

**Validation**:
- Must contain exactly one `/`
- Both vendor and name must be non-empty
- Invalid format returns descriptive error (FR-008)

---

### LocaleCode

Represents a locale identifier in "xx_YY" format (ISO 639-1 + ISO 3166-1).

| Field | Type | Description |
|-------|------|-------------|
| inner | `String` | Locale code (e.g., "en_US") |

**Derived Traits** (FR-007):
- `Debug`, `Clone`, `Eq`, `PartialEq`, `Hash`

**Methods**:
| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `fn new(code: &str) -> Result<Self>` | Validates format (FR-010) |
| `as_str` | `#[inline] fn as_str(&self) -> &str` | Returns code string (FR-002) |

**Validation** (FR-010):
- Must be exactly 5 characters
- Format: `[a-z]{2}_[A-Z]{2}`
- Examples: `en_US`, `nl_NL`, `de_DE`
- Invalid format returns: `"invalid locale format 'xxx': expected xx_YY (e.g., en_US)"`

---

### DeployJob

Combines theme and locale for parallel deployment execution.

| Field | Type | Description |
|-------|------|-------------|
| theme | `ThemeCode` | Target theme |
| locale | `LocaleCode` | Target locale |
| area | `Area` | frontend or adminhtml |

**Derived Traits**:
- `Debug`, `Clone`

---

### DeployStats

Tracks deployment progress with atomic counters.

| Field | Type | Description |
|-------|------|-------------|
| files_copied | `AtomicU64` | Total files copied |
| bytes_copied | `AtomicU64` | Total bytes copied |
| errors | `AtomicU64` | Error count |

**Concurrency** (FR-009):
- Uses `Ordering::Relaxed` for counter increments
- Progress bar updates batched every 100 files

---

### Theme

Represents a discovered Magento theme with parent chain.

| Field | Type | Description |
|-------|------|-------------|
| code | `ThemeCode` | Theme identifier |
| path | `PathBuf` | Filesystem path |
| parent | `Option<ThemeCode>` | Parent theme (if any) |
| area | `Area` | frontend or adminhtml |

**Derived Traits**:
- `Debug`, `Clone`

---

### Area

Enum for Magento design areas.

| Variant | Value |
|---------|-------|
| Frontend | `"frontend"` |
| Adminhtml | `"adminhtml"` |

**Derived Traits**:
- `Debug`, `Clone`, `Copy`, `Eq`, `PartialEq`

---

## Relationships

```
DeployJob
    ├── ThemeCode (1:1)
    ├── LocaleCode (1:1)
    └── Area (1:1)

Theme
    ├── ThemeCode (1:1)
    ├── Area (1:1)
    └── parent → ThemeCode (0..1)

DeployStats
    └── (standalone, shared across threads via Arc)
```

---

## State Transitions

### Deployment Lifecycle

```
[Idle] → discover_themes() → [Themes Discovered]
    ↓
[Themes Discovered] → create_jobs() → [Jobs Queued]
    ↓
[Jobs Queued] → rayon::par_iter() → [Deploying]
    ↓
[Deploying] → complete/error → [Done]
```

No persistent state; all state is runtime-only.
