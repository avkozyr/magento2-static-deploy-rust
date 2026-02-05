# Quickstart: magento-static-deploy (Rust)

**Feature**: 001-rust-port
**Date**: 2025-02-05

## Prerequisites

- Rust 1.75+ installed (`rustup update stable`)
- Magento 2.4+ installation
- Read access to Magento files
- Write access to `pub/static`

---

## Build

```bash
# Clone repository
cd /path/to/magento2-static-deploy-rust

# Build release binary
cargo build --release

# Binary location
./target/release/magento-static-deploy
```

---

## Installation

### Option 1: Copy binary to PATH
```bash
sudo cp target/release/magento-static-deploy /usr/local/bin/
```

### Option 2: Install with cargo
```bash
cargo install --path .
```

### Option 3: Use from build directory
```bash
./target/release/magento-static-deploy /var/www/magento
```

---

## Basic Usage

### Deploy all themes (from Magento root)
```bash
cd /var/www/magento
magento-static-deploy
```

### Deploy specific theme
```bash
magento-static-deploy -t Vendor/Hyva /var/www/magento
```

### Deploy with verbose output
```bash
magento-static-deploy -v /var/www/magento
```

---

## Common Workflows

### Development: Quick Hyva deploy
```bash
# Deploy single theme, single locale, verbose
magento-static-deploy -t Vendor/Hyva -l en_US -v .
```

### Production: Full multi-locale deploy
```bash
# All themes, all locales, max parallelism
magento-static-deploy -j 16 /var/www/magento
```

### CI/CD: Specific themes
```bash
# Deploy specific themes for pipeline
magento-static-deploy \
  -t Vendor/Hyva,Magento/backend \
  -l en_US,nl_NL,de_DE \
  /var/www/magento
```

---

## Verifying Installation

### Check version
```bash
magento-static-deploy --version
```

### Test deployment (dry-run style)
```bash
# Deploy single locale, check output
magento-static-deploy -t Vendor/Hyva -l en_US -v /var/www/magento
```

### Expected output
```
Deployed 8,234 files in 0.21s (39,209 files/sec)
  frontend/Vendor/Hyva/en_US: 8,234 files
```

---

## Troubleshooting

### "Magento root not found"
```bash
# Verify path exists
ls /var/www/magento/app/etc/env.php

# Use absolute path
magento-static-deploy /var/www/magento
```

### "Theme not found"
```bash
# List available themes
ls app/design/frontend/

# Check theme path
ls app/design/frontend/Vendor/Hyva/theme.xml
```

### "Permission denied"
```bash
# Check pub/static permissions
ls -la pub/static/

# Fix permissions
chmod -R u+w pub/static/
```

### Slow performance
```bash
# Check parallel workers
magento-static-deploy -j $(nproc) -v /var/www/magento

# Verify I/O isn't saturated
iostat -x 1
```

---

## Performance Expectations

| Files | Expected Time | Throughput |
|-------|---------------|------------|
| 10,000 | ~0.25s | 40,000 files/sec |
| 25,000 | ~0.6s | 40,000+ files/sec |
| 50,000 | ~1.2s | 40,000+ files/sec |

*Times measured on NVMe SSD with 8 CPU cores.*

---

## Next Steps

1. **Integrate with Magento CLI wrapper**:
   ```bash
   # Create wrapper script
   bin/magento-deploy-fast() {
     magento-static-deploy "$@"
   }
   ```

2. **Add to deployment scripts**:
   ```bash
   # In deploy.sh
   magento-static-deploy -j 16 /var/www/magento
   bin/magento cache:flush
   ```

3. **Monitor performance**:
   ```bash
   # Time deployment
   time magento-static-deploy -v /var/www/magento
   ```
