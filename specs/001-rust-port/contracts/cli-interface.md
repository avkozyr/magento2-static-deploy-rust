# CLI Interface Contract: magento-static-deploy

**Feature**: 001-rust-port
**Date**: 2025-02-05

## Command Overview

```
magento-static-deploy [OPTIONS] [MAGENTO_ROOT]
```

A high-performance static content deployment tool for Magento 2.

---

## Arguments

### Positional Arguments

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `MAGENTO_ROOT` | path | `.` (current directory) | Path to Magento 2 installation root |

---

## Options

### Theme Selection

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--area` | `-a` | string[] | `frontend,adminhtml` | Areas to deploy (comma-separated) |
| `--theme` | `-t` | string[] | (all discovered) | Themes to deploy in `Vendor/name` format |
| `--locale` | `-l` | string[] | `en_US` | Locales to deploy (comma-separated) |

### Execution Control

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--jobs` | `-j` | integer | (CPU cores) | Number of parallel workers |
| `--verbose` | `-v` | flag | false | Enable detailed progress output |
| `--help` | `-h` | flag | - | Show help message |
| `--version` | `-V` | flag | - | Show version information |

---

## Usage Examples

### Deploy all themes for all areas (default)
```bash
magento-static-deploy /var/www/magento
```

### Deploy specific theme and locale
```bash
magento-static-deploy -t Vendor/Hyva -l nl_NL /var/www/magento
```

### Deploy with maximum parallelism
```bash
magento-static-deploy -j 16 -v /var/www/magento
```

### Deploy frontend only
```bash
magento-static-deploy -a frontend /var/www/magento
```

### Deploy multiple themes and locales
```bash
magento-static-deploy -t Vendor/Hyva,Magento/backend -l en_US,nl_NL,de_DE /var/www/magento
```

---

## Output Format

### Standard Output (stdout)

On success, outputs deployment summary:
```
Deployed 12,345 files in 0.31s (39,823 files/sec)
  frontend/Vendor/Hyva/en_US: 8,234 files
  frontend/Vendor/Hyva/nl_NL: 4,111 files
```

### Verbose Output (stderr with `-v`)

Progress updates during deployment:
```
[1/4] Deploying frontend/Vendor/Hyva/en_US...
[2/4] Deploying frontend/Vendor/Hyva/nl_NL...
[3/4] Deploying adminhtml/Magento/backend/en_US...
[4/4] Deploying adminhtml/Magento/backend/nl_NL...
```

### Error Output (stderr)

Errors are written to stderr with context:
```
Error: Theme not found: Vendor/NonExistent
  Path checked: /var/www/magento/app/design/frontend/Vendor/NonExistent
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success - all themes deployed |
| 1 | Partial failure - some themes failed, others succeeded |
| 2 | Complete failure - no themes deployed |
| 130 | Interrupted - user pressed Ctrl+C |

---

## Signal Handling

| Signal | Behavior |
|--------|----------|
| `SIGINT` (Ctrl+C) | Graceful shutdown within 2 seconds, no partial files left |
| `SIGTERM` | Same as SIGINT |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RAYON_NUM_THREADS` | Override thread pool size (alternative to `--jobs`) |

---

## Compatibility

- **Magento**: 2.4+
- **Platforms**: Linux, macOS
- **Shell**: Works in bash, zsh, sh

---

## Go Tool Compatibility

This tool maintains argument compatibility with the Go implementation:

| Go Flag | Rust Equivalent | Notes |
|---------|-----------------|-------|
| `--area` | `--area` / `-a` | Same |
| `--theme` | `--theme` / `-t` | Same |
| `--locale` | `--locale` / `-l` | Same |
| `--jobs` | `--jobs` / `-j` | Same |
| `--verbose` | `--verbose` / `-v` | Same |
| positional | positional | Same |

Drop-in replacement: existing scripts work without modification.
