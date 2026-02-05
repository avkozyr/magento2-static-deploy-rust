# CLI Interface Contract

**Feature**: 002-rust-optimizations
**Date**: 2025-02-05

## Overview

No changes to CLI interface. All optimizations are internal.

## Existing Interface (unchanged)

```text
magento-static-deploy [MAGENTO_ROOT] [OPTIONS]

Arguments:
  [MAGENTO_ROOT]  Magento root directory [default: .]

Options:
  -a, --area <AREA>      Areas to deploy (comma-separated) [default: frontend,adminhtml]
  -t, --theme <THEME>    Themes to deploy in Vendor/name format (comma-separated)
  -l, --locale <LOCALE>  Locales to deploy (comma-separated) [default: en_US]
  -j, --jobs <JOBS>      Number of parallel workers [default: num_cpus]
  -v, --verbose          Enable verbose output
  -h, --help             Print help
  -V, --version          Print version
```

## Error Message Changes

### Before (FR-008)
```text
Error: Failed to copy file
```

### After (FR-008)
```text
Error: Failed to copy /src/path/file.js to /dest/path/file.js: Permission denied
```

## Validation Changes

### Locale Format (FR-010)

Warning for non-standard locale formats:

```text
Warning: Locale 'english' does not match expected format xx_YY (e.g., en_US)
```

Non-blocking - deployment continues.

## Performance Changes (invisible to user)

All optimizations are internal and do not change CLI behavior:

- FR-001: Path construction (no visible change)
- FR-002: Inline hints (no visible change)
- FR-003: XML parsing (no visible change)
- FR-004: Thread pool (may see different thread usage in verbose mode)
- FR-005: Arc handling (no visible change)
- FR-006: Pre-allocation (no visible change)
- FR-007: Derive traits (no visible change)
- FR-009: Progress batching (progress bar updates less frequently but still accurate)
- FR-011: Buffer size (no visible change)
- FR-012: Documentation (no visible change)
- FR-013: Tests (no visible change)

## Exit Codes (unchanged)

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Partial failure (some themes failed) |
| 2 | Complete failure or invalid arguments |
| 130 | Cancelled by user (Ctrl+C) |
