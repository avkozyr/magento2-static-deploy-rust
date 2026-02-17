//! File copying operations with buffered I/O for optimal performance.
//!
//! Uses 64KB buffers for improved NVMe SSD throughput compared to
//! the default 8KB buffer in `std::fs::copy`. File copying within
//! directories is parallelized using Rayon for maximum throughput.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::error::DeployError;

/// Buffer size for file copy operations (64KB for optimal NVMe performance).
/// Modern NVMe SSDs benefit from larger transfer sizes.
const COPY_BUFFER_SIZE: usize = 64 * 1024;

// Thread-local buffer pool to avoid allocation per file copy
thread_local! {
    static COPY_BUFFER: RefCell<Vec<u8>> = RefCell::new(vec![0u8; COPY_BUFFER_SIZE]);
}

/// Static HashSet for O(1) dev extension lookup
static DEV_EXTENSIONS_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
static DEV_FILES_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
static DEV_DIRECTORIES_SET: OnceLock<HashSet<&'static str>> = OnceLock::new();

fn dev_extensions() -> &'static HashSet<&'static str> {
    DEV_EXTENSIONS_SET.get_or_init(|| DEV_EXTENSIONS.iter().copied().collect())
}

fn dev_files() -> &'static HashSet<&'static str> {
    DEV_FILES_SET.get_or_init(|| DEV_FILES.iter().copied().collect())
}

fn dev_directories() -> &'static HashSet<&'static str> {
    DEV_DIRECTORIES_SET.get_or_init(|| DEV_DIRECTORIES.iter().copied().collect())
}

/// Extensions to exclude when --exclude-dev is enabled (matches Go/PHP behavior)
const DEV_EXTENSIONS: &[&str] = &[
    "ts",           // TypeScript source
    "tsx",          // TypeScript JSX
    "mts",          // TypeScript module
    "cts",          // TypeScript CommonJS module
    "less",         // LESS source
    "scss",         // SASS source
    "sass",         // SASS source
    "md",           // Markdown docs
    "markdown",     // Markdown docs
    "yml",          // YAML configs
    "yaml",         // YAML configs
    "lock",         // Lock files (package-lock.json, etc.)
    "npmignore",    // NPM ignore
    "gitignore",    // Git ignore
    "eslintrc",     // ESLint config
    "prettierrc",   // Prettier config
    "editorconfig", // Editor config
    "jshintrc",     // JSHint config
    "nycrc",        // NYC coverage config
    "babelrc",      // Babel config
    "flowconfig",   // Flow config
];

/// Files to exclude when --exclude-dev is enabled
const DEV_FILES: &[&str] = &[
    // Package managers
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "composer.json",
    "composer.lock",
    // TypeScript
    "tsconfig.json",
    "tsconfig.base.json",
    "tsconfig.build.json",
    // Docs
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "MIT-LICENSE",
    "README",
    "README.md",
    "README.txt",
    "CHANGELOG",
    "CHANGELOG.md",
    "HISTORY.md",
    "CONTRIBUTING.md",
    // Config files
    ".gitignore",
    ".npmignore",
    ".npmrc",
    ".yarnrc",
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.json",
    ".eslintrc.cjs",
    ".prettierrc",
    ".prettierrc.js",
    ".prettierrc.json",
    ".editorconfig",
    ".jshintrc",
    ".babelrc",
    ".babelrc.js",
    ".babelrc.json",
    "babel.config.js",
    "babel.config.json",
    ".nycrc",
    ".nycrc.json",
    "jest.config.js",
    "jest.config.json",
    "karma.conf.js",
    "webpack.config.js",
    "rollup.config.js",
    "vite.config.js",
    "vite.config.ts",
    ".browserslistrc",
    ".stylelintrc",
    ".stylelintrc.json",
    "Makefile",
    "Gruntfile.js",
    "Gulpfile.js",
];

/// Directories to exclude when --exclude-dev is enabled
const DEV_DIRECTORIES: &[&str] = &["node_modules", ".git", ".svn", ".hg"];

/// Check if a file should be excluded based on extension, name, or directory
/// By default, dev files are excluded. Use include_dev=true to include them.
/// Uses O(1) HashSet lookups and zero-allocation case-insensitive comparison.
#[inline]
fn should_exclude_file(path: &Path, include_dev: bool) -> bool {
    // If include_dev is true, don't exclude anything
    if include_dev {
        return false;
    }

    let dirs = dev_directories();
    let files = dev_files();
    let exts = dev_extensions();

    // Check if any parent directory is in DEV_DIRECTORIES
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(name_str) = name.to_str() {
                if dirs.contains(name_str) {
                    return true;
                }
            }
        }
    }

    // Check file name (O(1) lookup)
    if let Some(file_name) = path.file_name().and_then(OsStr::to_str) {
        if files.contains(file_name) {
            return true;
        }
    }

    // Check extension using case-insensitive comparison (zero allocation)
    if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        // Use eq_ignore_ascii_case for zero-allocation comparison
        for &dev_ext in exts.iter() {
            if ext.eq_ignore_ascii_case(dev_ext) {
                return true;
            }
        }
    }

    false
}

/// Check if an IO error is a disk full error (ENOSPC)
#[inline]
fn is_disk_full_error(e: &std::io::Error) -> bool {
    // Use ErrorKind::StorageFull if available (Rust 1.83+), fallback to raw errno
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(28) // ENOSPC on Unix
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Copy a single file from src to dst using thread-local buffer pool
pub fn copy_file(src: &Path, dst: &Path) -> Result<u64, DeployError> {
    // Create parent directory if needed
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                if is_disk_full_error(&e) {
                    return DeployError::DiskFull {
                        path: parent.to_path_buf(),
                    };
                }
                DeployError::CreateDirFailed {
                    path: parent.to_path_buf(),
                    source: e,
                }
            })?;
        }
    }

    let src_file = File::open(src).map_err(|e| DeployError::CopyFailed {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;
    let dst_file = File::create(dst).map_err(|e| {
        if is_disk_full_error(&e) {
            return DeployError::DiskFull {
                path: dst.to_path_buf(),
            };
        }
        DeployError::CopyFailed {
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
            source: e,
        }
    })?;

    let mut reader = BufReader::with_capacity(COPY_BUFFER_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(COPY_BUFFER_SIZE, dst_file);

    // Use thread-local buffer to avoid allocation per file
    let total_bytes = COPY_BUFFER.with(|buf| -> Result<u64, DeployError> {
        let mut buffer = buf.borrow_mut();
        let mut total = 0u64;

        loop {
            let bytes_read = reader
                .read(&mut buffer[..])
                .map_err(|e| DeployError::CopyFailed {
                    src: src.to_path_buf(),
                    dst: dst.to_path_buf(),
                    source: e,
                })?;

            if bytes_read == 0 {
                break;
            }

            writer.write_all(&buffer[..bytes_read]).map_err(|e| {
                if is_disk_full_error(&e) {
                    return DeployError::DiskFull {
                        path: dst.to_path_buf(),
                    };
                }
                DeployError::CopyFailed {
                    src: src.to_path_buf(),
                    dst: dst.to_path_buf(),
                    source: e,
                }
            })?;

            total += bytes_read as u64;
        }

        Ok(total)
    })?;

    writer.flush().map_err(|e| DeployError::CopyFailed {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;

    Ok(total_bytes)
}

/// Internal implementation for parallel directory copying
/// skip_existing: if true, skip files that already exist at destination
fn copy_directory_impl(
    src: &Path,
    dst: &Path,
    shutdown: &AtomicBool,
    include_dev: bool,
    skip_existing: bool,
) -> Result<(u64, u64), DeployError> {
    // Check for early cancellation
    if shutdown.load(Ordering::Relaxed) {
        return Err(DeployError::Cancelled);
    }

    // Collect all file entries first for parallel processing
    let entries: Vec<_> = WalkDir::new(src)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| !should_exclude_file(e.path(), include_dev))
        .collect();

    // Atomic counters for parallel aggregation
    let files_copied = AtomicU64::new(0);
    let bytes_copied = AtomicU64::new(0);

    // Process files in parallel using Rayon
    let result: Result<(), DeployError> = entries.par_iter().try_for_each(|entry| {
        // Check for cancellation
        if shutdown.load(Ordering::Relaxed) {
            return Err(DeployError::Cancelled);
        }

        let src_path = entry.path();
        let relative = src_path.strip_prefix(src).unwrap_or(src_path);
        let dst_path = dst.join(relative);

        // Skip if destination already exists and skip_existing is true
        if skip_existing {
            // Use create_new to atomically check existence and create in one syscall
            if let Some(parent) = dst_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent).map_err(|e| {
                        if is_disk_full_error(&e) {
                            return DeployError::DiskFull {
                                path: parent.to_path_buf(),
                            };
                        }
                        DeployError::CreateDirFailed {
                            path: parent.to_path_buf(),
                            source: e,
                        }
                    })?;
                }
            }

            // Try to create file exclusively - if it exists, skip
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst_path)
            {
                Ok(dst_file) => {
                    // File created, now copy content
                    let bytes = copy_file_to_handle(src_path, dst_file)?;
                    files_copied.fetch_add(1, Ordering::Relaxed);
                    bytes_copied.fetch_add(bytes, Ordering::Relaxed);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // File exists, skip it (higher priority source already copied)
                }
                Err(e) => {
                    if is_disk_full_error(&e) {
                        return Err(DeployError::DiskFull {
                            path: dst_path.clone(),
                        });
                    }
                    return Err(DeployError::CopyFailed {
                        src: src_path.to_path_buf(),
                        dst: dst_path,
                        source: e,
                    });
                }
            }
        } else {
            // Normal copy (overwrite if exists)
            let bytes = copy_file(src_path, &dst_path)?;
            files_copied.fetch_add(1, Ordering::Relaxed);
            bytes_copied.fetch_add(bytes, Ordering::Relaxed);
        }

        Ok(())
    });

    result?;

    Ok((
        files_copied.load(Ordering::Relaxed),
        bytes_copied.load(Ordering::Relaxed),
    ))
}

/// Copy file content to an already-opened file handle
fn copy_file_to_handle(src: &Path, dst_file: File) -> Result<u64, DeployError> {
    let src_file = File::open(src).map_err(|e| DeployError::CopyFailed {
        src: src.to_path_buf(),
        dst: src.to_path_buf(), // Not ideal but we don't have dst path here
        source: e,
    })?;

    let mut reader = BufReader::with_capacity(COPY_BUFFER_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(COPY_BUFFER_SIZE, dst_file);

    // Use thread-local buffer
    let total_bytes = COPY_BUFFER.with(|buf| -> Result<u64, DeployError> {
        let mut buffer = buf.borrow_mut();
        let mut total = 0u64;

        loop {
            let bytes_read = reader.read(&mut buffer[..]).map_err(DeployError::Io)?;

            if bytes_read == 0 {
                break;
            }

            writer
                .write_all(&buffer[..bytes_read])
                .map_err(DeployError::Io)?;

            total += bytes_read as u64;
        }

        Ok(total)
    })?;

    writer.flush().map_err(DeployError::Io)?;

    Ok(total_bytes)
}

/// Copy directory recursively, returns (files_copied, bytes_copied)
#[allow(dead_code)]
pub fn copy_directory(
    src: &Path,
    dst: &Path,
    shutdown: &AtomicBool,
    include_dev: bool,
) -> Result<(u64, u64), DeployError> {
    copy_directory_impl(src, dst, shutdown, include_dev, false)
}

/// Copy directory with override semantics (skip existing files)
/// Uses parallel file copying with Rayon for maximum throughput
pub fn copy_directory_with_overrides(
    src: &Path,
    dst: &Path,
    shutdown: &AtomicBool,
    include_dev: bool,
) -> Result<(u64, u64), DeployError> {
    copy_directory_impl(src, dst, shutdown, include_dev, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== should_exclude_file tests ====================

    #[test]
    fn test_should_exclude_file_typescript_extension() {
        let path = Path::new("/some/path/file.ts");
        assert!(should_exclude_file(path, false));
        assert!(!should_exclude_file(path, true));
    }

    #[test]
    fn test_should_exclude_file_less_extension() {
        let path = Path::new("/some/path/styles.less");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_scss_extension() {
        let path = Path::new("/some/path/styles.scss");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_markdown_extension() {
        let path = Path::new("/some/path/README.md");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_package_json() {
        let path = Path::new("/some/path/package.json");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_tsconfig() {
        let path = Path::new("/some/path/tsconfig.json");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_node_modules_directory() {
        let path = Path::new("/some/node_modules/package/index.js");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_git_directory() {
        let path = Path::new("/some/.git/config");
        assert!(should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_allowed_js() {
        let path = Path::new("/some/path/app.js");
        assert!(!should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_allowed_css() {
        let path = Path::new("/some/path/styles.css");
        assert!(!should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_allowed_html() {
        let path = Path::new("/some/path/index.html");
        assert!(!should_exclude_file(path, false));
    }

    #[test]
    fn test_should_exclude_file_include_dev_allows_all() {
        let ts_path = Path::new("/some/path/file.ts");
        let md_path = Path::new("/some/path/README.md");
        let node_path = Path::new("/some/node_modules/pkg/index.js");

        assert!(!should_exclude_file(ts_path, true));
        assert!(!should_exclude_file(md_path, true));
        assert!(!should_exclude_file(node_path, true));
    }

    #[test]
    fn test_should_exclude_file_case_insensitive_extension() {
        let path = Path::new("/some/path/file.TS");
        assert!(should_exclude_file(path, false));
    }

    // ==================== copy_file tests ====================

    #[test]
    fn test_copy_file_success() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("dest.txt");

        fs::write(&src, "test content").unwrap();

        let bytes = copy_file(&src, &dst).unwrap();

        assert_eq!(bytes, 12);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "test content");
    }

    #[test]
    fn test_copy_file_creates_parent_directories() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("nested/deep/dest.txt");

        fs::write(&src, "content").unwrap();

        let bytes = copy_file(&src, &dst).unwrap();

        assert_eq!(bytes, 7);
        assert!(dst.exists());
    }

    #[test]
    fn test_copy_file_source_not_found() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("nonexistent.txt");
        let dst = temp.path().join("dest.txt");

        let result = copy_file(&src, &dst);

        assert!(result.is_err());
    }

    #[test]
    fn test_copy_file_large_file() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("large.bin");
        let dst = temp.path().join("large_copy.bin");

        // Create 1MB file
        let data = vec![0xABu8; 1024 * 1024];
        fs::write(&src, &data).unwrap();

        let bytes = copy_file(&src, &dst).unwrap();

        assert_eq!(bytes, 1024 * 1024);
        assert_eq!(fs::read(&dst).unwrap(), data);
    }

    #[test]
    fn test_copy_file_empty_file() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("empty.txt");
        let dst = temp.path().join("empty_copy.txt");

        fs::write(&src, "").unwrap();

        let bytes = copy_file(&src, &dst).unwrap();

        assert_eq!(bytes, 0);
        assert!(dst.exists());
    }

    // ==================== copy_directory tests ====================

    #[test]
    fn test_copy_directory_success() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file1.txt"), "content1").unwrap();
        fs::write(src.join("file2.txt"), "content2").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, bytes) = copy_directory(&src, &dst, &shutdown, true).unwrap();

        assert_eq!(files, 2);
        assert_eq!(bytes, 16);
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("file2.txt").exists());
    }

    #[test]
    fn test_copy_directory_excludes_dev_files() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("app.js"), "code").unwrap();
        fs::write(src.join("app.ts"), "typescript").unwrap();
        fs::write(src.join("package.json"), "{}").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, _) = copy_directory(&src, &dst, &shutdown, false).unwrap();

        // Only app.js should be copied (not .ts or package.json)
        assert_eq!(files, 1);
        assert!(dst.join("app.js").exists());
        assert!(!dst.join("app.ts").exists());
        assert!(!dst.join("package.json").exists());
    }

    #[test]
    fn test_copy_directory_includes_dev_files() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("app.js"), "code").unwrap();
        fs::write(src.join("app.ts"), "typescript").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, _) = copy_directory(&src, &dst, &shutdown, true).unwrap();

        assert_eq!(files, 2);
        assert!(dst.join("app.js").exists());
        assert!(dst.join("app.ts").exists());
    }

    #[test]
    fn test_copy_directory_cancellation() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file.txt"), "content").unwrap();

        let shutdown = AtomicBool::new(true);
        let result = copy_directory(&src, &dst, &shutdown, true);

        assert!(matches!(result, Err(DeployError::Cancelled)));
    }

    #[test]
    fn test_copy_directory_nested_structure() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(src.join("a/b/c")).unwrap();
        fs::write(src.join("a/file1.txt"), "1").unwrap();
        fs::write(src.join("a/b/file2.txt"), "2").unwrap();
        fs::write(src.join("a/b/c/file3.txt"), "3").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, _) = copy_directory(&src, &dst, &shutdown, true).unwrap();

        assert_eq!(files, 3);
        assert!(dst.join("a/file1.txt").exists());
        assert!(dst.join("a/b/file2.txt").exists());
        assert!(dst.join("a/b/c/file3.txt").exists());
    }

    // ==================== copy_directory_with_overrides tests ====================

    #[test]
    fn test_copy_directory_with_overrides_skips_existing() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("file.txt"), "new content").unwrap();
        fs::write(dst.join("file.txt"), "existing").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, _) = copy_directory_with_overrides(&src, &dst, &shutdown, true).unwrap();

        // Should skip existing file
        assert_eq!(files, 0);
        assert_eq!(
            fs::read_to_string(dst.join("file.txt")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn test_copy_directory_with_overrides_copies_new() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("new.txt"), "new file").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, bytes) = copy_directory_with_overrides(&src, &dst, &shutdown, true).unwrap();

        assert_eq!(files, 1);
        assert_eq!(bytes, 8);
        assert!(dst.join("new.txt").exists());
    }

    #[test]
    fn test_copy_directory_with_overrides_cancellation() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file.txt"), "content").unwrap();

        let shutdown = AtomicBool::new(true);
        let result = copy_directory_with_overrides(&src, &dst, &shutdown, true);

        assert!(matches!(result, Err(DeployError::Cancelled)));
    }

    #[test]
    fn test_copy_directory_with_overrides_excludes_dev() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("app.js"), "code").unwrap();
        fs::write(src.join("app.ts"), "typescript").unwrap();

        let shutdown = AtomicBool::new(false);
        let (files, _) = copy_directory_with_overrides(&src, &dst, &shutdown, false).unwrap();

        assert_eq!(files, 1);
        assert!(dst.join("app.js").exists());
        assert!(!dst.join("app.ts").exists());
    }
}
