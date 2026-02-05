use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::copier::copy_directory_with_overrides;
use crate::error::DeployError;
use crate::scanner::{collect_file_sources, FileSource};
use crate::theme::{resolve_parent_chain, LocaleCode, Theme, ThemeType};

/// A deployment job combining theme, locale, and area
/// Uses Arc for efficient sharing across parallel workers without cloning
#[derive(Debug, Clone)]
pub struct DeployJob {
    /// Theme to deploy (shared reference)
    pub theme: Arc<Theme>,
    /// Locale code (type-safe wrapper)
    pub locale: LocaleCode,
}

/// Result of a deployment job
#[derive(Debug)]
#[allow(dead_code)]
pub struct DeployResult {
    /// Which job this result belongs to
    pub job: DeployJob,
    /// Success or failure status
    pub status: DeployStatus,
    /// Number of files copied
    pub file_count: u64,
    /// Time taken for this job
    pub duration: Duration,
}

/// Deployment outcome
#[derive(Debug)]
pub enum DeployStatus {
    /// Successfully deployed
    Success,
    /// Failed with error
    Failed(DeployError),
    /// Cancelled by user (SIGINT)
    Cancelled,
    /// Delegated to bin/magento (Luma themes)
    Delegated,
}

/// Cache-line aligned atomic counter to prevent false sharing
/// Each counter is on its own 64-byte cache line
#[repr(align(64))]
pub struct CacheAlignedAtomic(pub AtomicU64);

impl CacheAlignedAtomic {
    pub const fn new(val: u64) -> Self {
        Self(AtomicU64::new(val))
    }
}

/// Global counters for progress tracking
/// Cache line padding prevents false sharing between counters
/// when updated from different threads
pub struct DeployStats {
    pub files_copied: CacheAlignedAtomic,
    pub bytes_copied: CacheAlignedAtomic,
    pub errors: CacheAlignedAtomic,
}

impl DeployStats {
    pub fn new() -> Self {
        Self {
            files_copied: CacheAlignedAtomic::new(0),
            bytes_copied: CacheAlignedAtomic::new(0),
            errors: CacheAlignedAtomic::new(0),
        }
    }
}

impl Default for DeployStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Build output path for theme deployment
pub fn output_path_for_theme(magento_root: &Path, theme: &Theme, locale: &LocaleCode) -> PathBuf {
    magento_root
        .join("pub")
        .join("static")
        .join(theme.area.as_str())
        .join(&theme.vendor)
        .join(&theme.name)
        .join(locale.as_str())
}

/// Read deployed version from pub/static/deployed_version.txt
#[allow(dead_code)]
pub fn read_deployed_version(magento_root: &Path) -> Option<String> {
    let version_file = magento_root
        .join("pub")
        .join("static")
        .join("deployed_version.txt");

    fs::read_to_string(version_file)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Deploy a single theme for a single locale
pub fn deploy_theme(
    job: &DeployJob,
    all_themes: &[Theme],
    magento_root: &Path,
    shutdown: &AtomicBool,
    stats: &DeployStats,
    verbose: bool,
    include_dev: bool,
) -> DeployResult {
    let start = Instant::now();

    // Check for Luma theme - delegate to bin/magento
    if job.theme.theme_type == ThemeType::Luma {
        return delegate_to_magento(job, magento_root, start, verbose);
    }

    // Resolve parent chain
    let parent_chain = resolve_parent_chain(&job.theme, all_themes);

    // Collect all file sources
    let sources = collect_file_sources(&job.theme, &parent_chain, magento_root);

    // Build output path
    let output_path = output_path_for_theme(magento_root, &job.theme, &job.locale);

    // Copy files from each source
    let mut total_files = 0u64;

    for source in sources {
        if shutdown.load(Ordering::Relaxed) {
            return DeployResult {
                job: job.clone(),
                status: DeployStatus::Cancelled,
                file_count: total_files,
                duration: start.elapsed(),
            };
        }

        let (src_path, dest_subpath) = match &source {
            FileSource::ThemeWeb { path, .. } => (path.clone(), PathBuf::new()),
            FileSource::Library { path } => (path.clone(), PathBuf::new()),
            FileSource::VendorModule { module, path } => (path.clone(), PathBuf::from(module)),
            FileSource::ThemeModuleOverride { module, path, .. } => {
                (path.clone(), PathBuf::from(module))
            }
        };

        let dest = if dest_subpath.as_os_str().is_empty() {
            output_path.clone()
        } else {
            output_path.join(dest_subpath)
        };

        match copy_directory_with_overrides(&src_path, &dest, shutdown, include_dev) {
            Ok((files, bytes)) => {
                total_files += files;
                stats.files_copied.0.fetch_add(files, Ordering::Relaxed);
                stats.bytes_copied.0.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(DeployError::Cancelled) => {
                return DeployResult {
                    job: job.clone(),
                    status: DeployStatus::Cancelled,
                    file_count: total_files,
                    duration: start.elapsed(),
                };
            }
            Err(e) => {
                stats.errors.0.fetch_add(1, Ordering::Relaxed);
                return DeployResult {
                    job: job.clone(),
                    status: DeployStatus::Failed(e),
                    file_count: total_files,
                    duration: start.elapsed(),
                };
            }
        }
    }

    DeployResult {
        job: job.clone(),
        status: DeployStatus::Success,
        file_count: total_files,
        duration: start.elapsed(),
    }
}

/// Delegate Luma theme to bin/magento
fn delegate_to_magento(
    job: &DeployJob,
    magento_root: &Path,
    start: Instant,
    verbose: bool,
) -> DeployResult {
    let magento_bin = magento_root.join("bin").join("magento");

    let result = Command::new(&magento_bin)
        .args([
            "setup:static-content:deploy",
            "--area",
            job.theme.area.as_str(),
            "--theme",
            &job.theme.full_name(),
            job.locale.as_str(),
        ])
        .current_dir(magento_root)
        .output();

    match result {
        Ok(output) => {
            if verbose {
                if !output.stdout.is_empty() {
                    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                }
            }

            if output.status.success() {
                DeployResult {
                    job: job.clone(),
                    status: DeployStatus::Delegated,
                    file_count: 0,
                    duration: start.elapsed(),
                }
            } else {
                let code = output.status.code().unwrap_or(-1);
                DeployResult {
                    job: job.clone(),
                    status: DeployStatus::Failed(DeployError::MagentoFailed {
                        code,
                        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    }),
                    file_count: 0,
                    duration: start.elapsed(),
                }
            }
        }
        Err(e) => DeployResult {
            job: job.clone(),
            status: DeployStatus::Failed(DeployError::Io(e)),
            file_count: 0,
            duration: start.elapsed(),
        },
    }
}

/// Generate job matrix for all theme×locale combinations
/// Uses Arc to share theme references efficiently
pub fn job_matrix(themes: &[Theme], locales: &[LocaleCode]) -> Vec<DeployJob> {
    let mut jobs = Vec::with_capacity(themes.len() * locales.len());

    for theme in themes {
        let theme_arc = Arc::new(theme.clone());
        for locale in locales {
            jobs.push(DeployJob {
                theme: Arc::clone(&theme_arc),
                locale: locale.clone(),
            });
        }
    }

    jobs
}

/// Collect and aggregate results from parallel jobs
pub fn collect_results(results: Vec<DeployResult>) -> (Vec<DeployResult>, bool, bool) {
    let mut all_results = Vec::with_capacity(results.len());
    let mut has_success = false;
    let mut has_failure = false;

    for result in results {
        match &result.status {
            DeployStatus::Success | DeployStatus::Delegated => has_success = true,
            DeployStatus::Failed(_) => has_failure = true,
            DeployStatus::Cancelled => {}
        }
        all_results.push(result);
    }

    (all_results, has_success, has_failure)
}
