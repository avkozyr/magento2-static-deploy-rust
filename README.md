# Magento Hyva Static Content Deployer (Rust)

High-performance static content deployment for Magento 2 Hyva, written in Rust using Speckit and Claude.

## Background

This project is a Rust port of [Magento Static Content Deployer (Go)](https://github.com/elgentos/magento2-static-deploy), a Go-based tool for fast Magento static content deployment by Elgentos. The Rust implementation improves on the original with:

- **Faster throughput**
- **Smaller binary**
- **Correct multi-locale handling** (separate directories per locale - Go creates combined directory)

## Features

- **Parallel**: Deploys multiple themes and locales concurrently
- **Smart**: Detects Hyva vs Luma themes automatically
- **Safe**: Graceful Ctrl+C cancellation, no partial files
- **Compatible**: Drop-in replacement for existing scripts

## Installation

### From Source

```bash
# Requires Rust 1.75+
cargo build --release

# Binary at target/release/magento-static-deploy
sudo cp target/release/magento-static-deploy /usr/local/bin/
```

### From Cargo

```bash
cargo install --path .
```

### Cross-Compilation (All Platforms)

Build release binaries for all target environments:

```bash
# macOS (native, if on Mac)
cargo build --release
cp target/release/magento-static-deploy magento-static-deploy-darwin-arm64

# Linux x86_64 (via Docker)
docker run --rm -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/magento-static-deploy-linux-amd64"

# Linux ARM64 (via Docker, for ddev/OrbStack)
docker run --rm --platform linux/arm64 -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/magento-static-deploy-linux-arm64"
```

#### Build All Script

```bash
#!/bin/bash
# build-all.sh - Build for all platforms

set -e

echo "Building for macOS ARM64..."
cargo build --release
cp target/release/magento-static-deploy dist/magento-static-deploy-darwin-arm64

echo "Building for Linux x86_64..."
docker run --rm -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/dist/magento-static-deploy-linux-amd64"

echo "Building for Linux ARM64..."
docker run --rm --platform linux/arm64 -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/dist/magento-static-deploy-linux-arm64"

echo "Done! Binaries in dist/"
ls -la dist/
```

## Usage

### Basic

```bash
# Deploy all themes (from Magento root)
cd /var/www/magento
magento-static-deploy

# Deploy specific theme
magento-static-deploy -t Vendor/Hyva /var/www/magento

# Deploy with verbose output
magento-static-deploy -v /var/www/magento
```

### Options

```
magento-static-deploy [OPTIONS] [MAGENTO_ROOT]

Arguments:
  [MAGENTO_ROOT]  Magento root directory [default: .]

Options:
  -a, --area <AREA>      Areas to deploy [default: frontend,adminhtml]
  -t, --theme <THEME>    Themes to deploy (Vendor/name format)
  -l, --locale <LOCALE>  Locales to deploy [default: en_US]
  -j, --jobs <JOBS>      Parallel workers [default: CPU cores]
  -v, --verbose          Enable progress output
  -d, --include-dev      Include dev files (.ts, .less, .md, node_modules)
  -h, --help             Print help
  -V, --version          Print version
```

## How It Works

### Hyva Themes (Fast Path)

For Hyva themes, the tool copies static files directly:

1. Discovers themes in `app/design/{area}/`
2. Resolves parent chain from `theme.xml`
3. Copies files from theme web directories
4. Applies module overrides
5. Outputs to `pub/static/{area}/{Vendor}/{theme}/{locale}/`

### Luma Themes (Fallback)

For Luma themes requiring LESS/RequireJS compilation:

1. Detects non-Hyva theme
2. Delegates to `bin/magento setup:static-content:deploy`
3. Reports result

### Benchmarks

Run micro-benchmarks with Criterion:

```bash
# Run all benchmarks
cargo bench

# Compare before/after optimization
cargo bench -- --save-baseline before
# ... make changes ...
cargo bench -- --baseline before
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for detailed benchmark documentation.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Partial failure (some themes failed) |
| 2 | Complete failure |
| 130 | Interrupted (Ctrl+C) |

## Requirements

- Rust 1.75+ (build only)
- Magento 2.4+
- Read access to Magento files
- Write access to `pub/static`

## License

MIT
