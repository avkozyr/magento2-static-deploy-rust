# Data Model: Rust Performance Optimizations

**Feature**: 002-rust-optimizations
**Date**: 2025-02-05

## Core Entities

### ThemeCode

Represents a Magento theme identifier in "Vendor/name" format.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| inner | Arc<str> | Shared string reference for zero-copy cloning |

**Derives** (to add):
- `PartialEq`, `Eq`: Value equality comparison
- `Hash`: Use in HashMaps/HashSets
- `Clone`: Already present

**Methods**:
| Method | Signature | Inline | Description |
|--------|-----------|--------|-------------|
| new | (vendor: &str, name: &str) -> Self | Yes | Construct from parts |
| parse | (s: &str) -> Option<Self> | Yes | Parse "Vendor/name" format |
| as_str | (&self) -> &str | Yes | Get inner string |
| vendor | (&self) -> &str | Yes | Extract vendor part |
| name | (&self) -> &str | Yes | Extract name part |

### LocaleCode

Represents a locale identifier in "xx_YY" format.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| inner | Arc<str> | Shared string reference for zero-copy cloning |

**Derives** (to add):
- `PartialEq`, `Eq`: Value equality comparison
- `Hash`: Use in HashMaps/HashSets
- `Clone`: Already present

**Methods**:
| Method | Signature | Inline | Description |
|--------|-----------|--------|-------------|
| new | (s: &str) -> Self | Yes | Construct from string |
| as_str | (&self) -> &str | Yes | Get inner string |
| is_valid_format | (&self) -> bool | Yes | Check xx_YY format |

### Theme

Represents a Magento theme with metadata.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| vendor | String | Vendor name (e.g., "Hyva") |
| name | String | Theme name (e.g., "default") |
| area | Area | Frontend or Adminhtml |
| path | PathBuf | Filesystem path to theme |
| parent | Option<ThemeCode> | Parent theme in inheritance |
| theme_type | ThemeType | Hyva or Luma |

**Derives** (to add):
- `PartialEq`, `Eq`: Theme comparison for tests

### DeployJob

Combines theme and locale for deployment.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| theme | Arc<Theme> | Shared theme reference |
| locale | LocaleCode | Locale to deploy |

**Optimization**: Pass by reference (`&DeployJob`) rather than cloning.

### DeployStats

Atomic counters for progress tracking.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| files_copied | CacheAlignedAtomic | Total files copied |
| bytes_copied | CacheAlignedAtomic | Total bytes copied |
| errors | CacheAlignedAtomic | Error count |

**Optimization**: Batch updates with thread-local counters.

### Error Types

Enhanced error variants with full context.

**CopyFailed** (updated):
| Field | Type | Description |
|-------|------|-------------|
| src | PathBuf | Source file path |
| dst | PathBuf | Destination file path |
| source | std::io::Error | Underlying I/O error |

## Relationships

```text
Theme 1 ──────< 0..1 ThemeCode (parent)
Theme 1 ──────< * DeployJob
DeployJob 1 ──── 1 LocaleCode
DeployStats ────< * DeployJob (shared across all)
```

## State Transitions

### DeployJob Lifecycle

```text
Created → Running → Completed
                  → Failed
                  → Cancelled
```

### File Copy Lifecycle

```text
Pending → Copying → Copied
                  → Failed (retry not implemented)
                  → Skipped (target newer)
```

## Validation Rules

| Entity | Rule | Enforcement |
|--------|------|-------------|
| ThemeCode | Must contain exactly one '/' | parse() returns None |
| LocaleCode | Should match xx_YY pattern | is_valid_format() warns |
| Theme | Must have valid theme.xml | discover_themes skips |
| DeployJob | Theme must exist | job_matrix validates |
