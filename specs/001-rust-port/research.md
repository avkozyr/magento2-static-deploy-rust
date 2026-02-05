# Research: Rust Port of Magento 2 Static Deploy

**Feature**: 001-rust-port
**Date**: 2025-02-05

## Executive Summary

This research consolidates findings for implementing a high-performance Magento 2 static content deployment tool in Rust, porting functionality from an existing Go implementation.

---

## 1. File Copy Performance in Rust

### Decision: Use `std::fs::copy` with fallback to `sendfile`/`copy_file_range`

### Rationale
- Rust's `std::fs::copy` uses platform-native zero-copy operations where available:
  - Linux: `copy_file_range` syscall (kernel 4.5+) or `sendfile`
  - macOS: `copyfile` with `COPYFILE_CLONE` for APFS cloning
- These are already zero-copy at the kernel level when supported
- For older systems, falls back to buffered copy with 64KB chunks

### Alternatives Considered
| Alternative | Rejected Because |
|-------------|------------------|
| `memmap2` + manual copy | Adds complexity, no benefit for typical file sizes (<1MB) |
| `io_uring` | Linux-only, adds async complexity, marginal gain for file copy |
| Custom buffer pools | `std::fs::copy` already optimal, premature optimization |

### Implementation Note
```rust
// Simple, optimal approach
std::fs::copy(src, dst)?;

// For very large files (>10MB), consider mmap:
if file_size > 10_000_000 {
    let mmap = unsafe { Mmap::map(&file)? };
    std::fs::write(dst, &mmap[..])?;
}
```

---

## 2. Parallel Processing Strategy

### Decision: Use Rayon with work-stealing thread pool

### Rationale
- Rayon provides zero-config parallelism with `par_iter()`
- Work-stealing automatically balances load across cores
- No channel management, mutex coordination, or manual thread spawning
- Matches Go's goroutine pool pattern with less boilerplate

### Alternatives Considered
| Alternative | Rejected Because |
|-------------|------------------|
| `tokio` async | File I/O is syscall-bound, not network-bound; async adds overhead |
| `crossbeam` channels | More manual setup than Rayon for CPU-bound work |
| `std::thread` pool | Rayon already provides optimized pool |

### Implementation Pattern
```rust
use rayon::prelude::*;

jobs.par_iter()
    .map(|job| deploy_theme(job))
    .collect::<Result<Vec<_>>>()?;
```

---

## 3. Theme Detection (Hyva vs Luma)

### Decision: Check for `Hyva_Theme` in `theme.xml` or parent chain containing `Hyva/`

### Rationale
- Hyva themes register `Hyva_Theme` module as dependency
- Parent chain includes `Hyva/reset` or `Hyva/default`
- Quick string search, no XML parsing library needed for detection

### Detection Logic
```rust
fn is_hyva_theme(theme_xml_content: &str, parent_chain: &[String]) -> bool {
    theme_xml_content.contains("Hyva_Theme") ||
    parent_chain.iter().any(|p| p.starts_with("Hyva/"))
}
```

---

## 4. XML Parsing for theme.xml

### Decision: Use `quick-xml` for streaming XML parsing

### Rationale
- `quick-xml` is zero-allocation streaming parser
- Only need to extract `<parent>` element from `theme.xml`
- No need for full DOM parsing (avoid `roxmltree` overhead)

### Alternatives Considered
| Alternative | Rejected Because |
|-------------|------------------|
| `roxmltree` | Full DOM allocation unnecessary for simple extraction |
| Regex | Fragile for XML, edge cases with CDATA/comments |
| `serde-xml-rs` | Overkill for single element extraction |

### Implementation
```rust
use quick_xml::Reader;
use quick_xml::events::Event;

fn parse_parent(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_parent = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"parent" => in_parent = true,
            Ok(Event::Text(e)) if in_parent => return Some(e.unescape().ok()?.into_owned()),
            Ok(Event::Eof) => return None,
            _ => {}
        }
    }
}
```

---

## 5. CLI Argument Parsing

### Decision: Use `clap` v4 with derive macros

### Rationale
- Industry standard for Rust CLI applications
- Derive macros reduce boilerplate
- Auto-generates help text and completions
- Type-safe argument validation

### CLI Structure
```rust
#[derive(Parser)]
#[command(name = "magento-static-deploy")]
struct Cli {
    /// Magento root directory
    #[arg(default_value = ".")]
    magento_root: PathBuf,

    /// Areas to deploy
    #[arg(short, long, value_delimiter = ',', default_value = "frontend,adminhtml")]
    area: Vec<String>,

    /// Themes to deploy (Vendor/name format)
    #[arg(short, long, value_delimiter = ',')]
    theme: Option<Vec<String>>,

    /// Locales to deploy
    #[arg(short, long, value_delimiter = ',', default_value = "en_US")]
    locale: Vec<String>,

    /// Number of parallel jobs
    #[arg(short, long, default_value_t = num_cpus::get())]
    jobs: usize,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}
```

---

## 6. Error Handling Strategy

### Decision: Use `anyhow` for application errors, `thiserror` for library errors

### Rationale
- `anyhow` provides `.context()` for error chaining
- `thiserror` generates `Display` and `Error` impls from derive
- Clear separation: public API uses typed errors, internal uses anyhow

### Error Types
```rust
#[derive(thiserror::Error, Debug)]
pub enum DeployError {
    #[error("Magento root not found: {path}")]
    RootNotFound { path: PathBuf },

    #[error("Theme not found: {theme}")]
    ThemeNotFound { theme: String },

    #[error("Failed to copy file {src} to {dst}")]
    CopyFailed {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
```

---

## 7. Signal Handling for Graceful Shutdown

### Decision: Use `ctrlc` crate with atomic flag

### Rationale
- Simple, cross-platform SIGINT/SIGTERM handling
- Atomic flag checked in worker loops
- Rayon respects early termination via `Result::Err`

### Implementation
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

let shutdown = Arc::new(AtomicBool::new(false));
let shutdown_clone = shutdown.clone();

ctrlc::set_handler(move || {
    shutdown_clone.store(true, Ordering::SeqCst);
})?;

// In worker loop:
if shutdown.load(Ordering::Relaxed) {
    return Err(anyhow!("Cancelled"));
}
```

---

## 8. Progress Tracking

### Decision: Use `AtomicU64` counters, print to stderr in verbose mode

### Rationale
- Atomics avoid mutex contention per constitution
- stderr keeps stdout clean for scripting
- Simple counter, no progress bar library needed

### Implementation
```rust
use std::sync::atomic::{AtomicU64, Ordering};

static FILE_COUNT: AtomicU64 = AtomicU64::new(0);

// In copy loop:
FILE_COUNT.fetch_add(1, Ordering::Relaxed);

// After completion:
let count = FILE_COUNT.load(Ordering::Relaxed);
eprintln!("Deployed {} files in {:.2}s", count, duration.as_secs_f64());
```

---

## 9. Directory Traversal

### Decision: Use `walkdir` with parallel iteration

### Rationale
- `walkdir` is mature, handles edge cases (symlinks, permissions)
- Can be wrapped with Rayon for parallel directory walking
- Configurable follow_links, max_depth, filters

### Implementation
```rust
use walkdir::WalkDir;

WalkDir::new(path)
    .follow_links(true)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.file_type().is_file())
    .collect::<Vec<_>>()
```

---

## 10. Release Build Optimization

### Decision: Use LTO, single codegen unit, panic=abort

### Rationale
- Per constitution: maximize performance in release builds
- LTO enables cross-module inlining
- Single codegen unit improves optimization scope
- `panic=abort` reduces binary size, faster than unwinding

### Cargo.toml Profile
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

---

## Dependencies Summary

| Crate | Version | Purpose |
|-------|---------|---------|
| `clap` | 4 | CLI argument parsing |
| `rayon` | 1.10 | Parallel processing |
| `walkdir` | 2 | Directory traversal |
| `quick-xml` | 0.31 | theme.xml parsing |
| `anyhow` | 1 | Error context |
| `thiserror` | 2 | Error types |
| `ctrlc` | 3 | Signal handling |
| `num_cpus` | 1 | Default job count |

---

## Benchmark Results (2025-02-05)

### Criterion Micro-Benchmarks

| Benchmark | Time | Throughput |
|-----------|------|------------|
| copy_file_1kb | 182 µs | - |
| copy_file_1mb | 160 µs | - |
| copy_directory/100 | 35.7 ms | 2,803 files/sec |
| copy_directory/500 | 128.4 ms | 3,894 files/sec |
| copy_directory/1000 | 259.5 ms | 3,854 files/sec |
| discover_themes_5 | 199 µs | - |

### Real-World Validation (Zuiver Store)

| Scenario | Files | Time | Throughput |
|----------|-------|------|------------|
| Single theme, single locale | 9,012 | 1.73s | 5,200 files/sec |
| Single theme, 3 locales (parallel) | 27,036 | 2.40s | 11,255 files/sec |

**Observations**:
- Parallel scaling: 2.16× throughput with 3× workload
- Real-world throughput exceeds micro-benchmark due to file caching
- I/O-bound workload benefits from more threads than CPU cores

### Binary Size

- Release binary: 898 KB (stripped, LTO, single codegen unit)

---

## Open Questions Resolved

All technical context questions resolved:
- ✅ File copy strategy: `std::fs::copy` (kernel zero-copy)
- ✅ Parallelism: Rayon work-stealing pool
- ✅ XML parsing: `quick-xml` streaming
- ✅ Error handling: anyhow + thiserror
- ✅ Signal handling: ctrlc + atomic flag
- ✅ Progress: AtomicU64 counters
- ✅ Performance validated: 5,200-11,255 files/sec
