use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::WalkDir;

use crate::error::DeployError;

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
const DEV_DIRECTORIES: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".hg",
];

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

    fs::copy(src, dst).map_err(|e| {
        // Check for disk full error (ENOSPC = 28 on Unix)
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
    })
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
