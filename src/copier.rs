//! File copying operations with buffered I/O for optimal performance.
//!
//! Uses 64KB buffers for improved NVMe SSD throughput compared to
//! the default 8KB buffer in `std::fs::copy`.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::WalkDir;

use crate::error::DeployError;

/// Buffer size for file copy operations (64KB for optimal NVMe performance).
/// Modern NVMe SSDs benefit from larger transfer sizes.
const COPY_BUFFER_SIZE: usize = 64 * 1024;

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
#[inline]
fn should_exclude_file(path: &Path, include_dev: bool) -> bool {
    // If include_dev is true, don't exclude anything
    if include_dev {
        return false;
    }

    // Check if any parent directory is in DEV_DIRECTORIES
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(name_str) = name.to_str() {
                if DEV_DIRECTORIES.contains(&name_str) {
                    return true;
                }
            }
        }
    }

    // Check file name
    if let Some(file_name) = path.file_name().and_then(OsStr::to_str) {
        if DEV_FILES.contains(&file_name) {
            return true;
        }
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        let ext_lower = ext.to_lowercase();
        if DEV_EXTENSIONS.contains(&ext_lower.as_str()) {
            return true;
        }
    }

    false
}

/// Copy a single file from src to dst
pub fn copy_file(src: &Path, dst: &Path) -> Result<u64, DeployError> {
    // Create parent directory if needed
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                // Check for disk full error
                if e.raw_os_error() == Some(28) {
                    // ENOSPC on Unix
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

    // Use 64KB buffer for optimal NVMe performance (FR-011)
    let src_file = File::open(src).map_err(|e| DeployError::CopyFailed {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;
    let dst_file = File::create(dst).map_err(|e| {
        if e.raw_os_error() == Some(28) {
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

    let mut buffer = [0u8; COPY_BUFFER_SIZE];
    let mut total_bytes = 0u64;

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| DeployError::CopyFailed {
                src: src.to_path_buf(),
                dst: dst.to_path_buf(),
                source: e,
            })?;

        if bytes_read == 0 {
            break;
        }

        writer.write_all(&buffer[..bytes_read]).map_err(|e| {
            if e.raw_os_error() == Some(28) {
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

        total_bytes += bytes_read as u64;
    }

    writer.flush().map_err(|e| DeployError::CopyFailed {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;

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
    let mut files_copied = 0u64;
    let mut bytes_copied = 0u64;

    for entry in WalkDir::new(src)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        // Check for cancellation
        if shutdown.load(Ordering::Relaxed) {
            return Err(DeployError::Cancelled);
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let src_path = entry.path();

        // Skip development files unless --include-dev is enabled
        if should_exclude_file(src_path, include_dev) {
            continue;
        }

        let relative = src_path.strip_prefix(src).unwrap_or(src_path);
        let dst_path = dst.join(relative);

        let bytes = copy_file(src_path, &dst_path)?;
        files_copied += 1;
        bytes_copied += bytes;
    }

    Ok((files_copied, bytes_copied))
}

/// Copy directory with override semantics (skip existing files)
pub fn copy_directory_with_overrides(
    src: &Path,
    dst: &Path,
    shutdown: &AtomicBool,
    include_dev: bool,
) -> Result<(u64, u64), DeployError> {
    let mut files_copied = 0u64;
    let mut bytes_copied = 0u64;

    for entry in WalkDir::new(src)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        // Check for cancellation
        if shutdown.load(Ordering::Relaxed) {
            return Err(DeployError::Cancelled);
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let src_path = entry.path();

        // Skip development files unless --include-dev is enabled
        if should_exclude_file(src_path, include_dev) {
            continue;
        }

        let relative = src_path.strip_prefix(src).unwrap_or(src_path);
        let dst_path = dst.join(relative);

        // Skip if destination already exists (higher priority source already copied)
        if dst_path.exists() {
            continue;
        }

        let bytes = copy_file(src_path, &dst_path)?;
        files_copied += 1;
        bytes_copied += bytes;
    }

    Ok((files_copied, bytes_copied))
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
