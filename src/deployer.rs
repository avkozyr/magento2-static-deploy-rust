//! Theme deployment orchestration for Magento 2.
//!
//! Handles parallel deployment of themes across locales, with support for:
//! - Hyva themes (direct file copy)
//! - Luma themes (delegation to bin/magento)
//! - Progress tracking with cache-aligned atomic counters

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Area, Theme, ThemeType};
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tempfile::TempDir;

    // ==================== CacheAlignedAtomic tests ====================

    #[test]
    fn test_cache_aligned_atomic_new() {
        let atomic = CacheAlignedAtomic::new(42);
        assert_eq!(atomic.0.load(Ordering::Relaxed), 42);
    }

    #[test]
    fn test_cache_aligned_atomic_fetch_add() {
        let atomic = CacheAlignedAtomic::new(10);
        atomic.0.fetch_add(5, Ordering::Relaxed);
        assert_eq!(atomic.0.load(Ordering::Relaxed), 15);
    }

    #[test]
    fn test_cache_aligned_atomic_alignment() {
        // Verify the alignment is 64 bytes
        assert_eq!(std::mem::align_of::<CacheAlignedAtomic>(), 64);
    }

    // ==================== DeployStats tests ====================

    #[test]
    fn test_deploy_stats_new() {
        let stats = DeployStats::new();
        assert_eq!(stats.files_copied.0.load(Ordering::Relaxed), 0);
        assert_eq!(stats.bytes_copied.0.load(Ordering::Relaxed), 0);
        assert_eq!(stats.errors.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_deploy_stats_default() {
        let stats = DeployStats::default();
        assert_eq!(stats.files_copied.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_deploy_stats_increment() {
        let stats = DeployStats::new();
        stats.files_copied.0.fetch_add(100, Ordering::Relaxed);
        stats.bytes_copied.0.fetch_add(1024, Ordering::Relaxed);
        stats.errors.0.fetch_add(2, Ordering::Relaxed);

        assert_eq!(stats.files_copied.0.load(Ordering::Relaxed), 100);
        assert_eq!(stats.bytes_copied.0.load(Ordering::Relaxed), 1024);
        assert_eq!(stats.errors.0.load(Ordering::Relaxed), 2);
    }

    // ==================== output_path_for_theme tests ====================

    #[test]
    fn test_output_path_for_theme_frontend() {
        let temp = TempDir::new().unwrap();
        let theme = Theme {
            vendor: "Hyva".to_string(),
            name: "default".to_string(),
            area: Area::Frontend,
            path: temp.path().to_path_buf(),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let locale = LocaleCode::new("en_US");

        let path = output_path_for_theme(temp.path(), &theme, &locale);

        assert!(path.ends_with("pub/static/frontend/Hyva/default/en_US"));
    }

    #[test]
    fn test_output_path_for_theme_adminhtml() {
        let temp = TempDir::new().unwrap();
        let theme = Theme {
            vendor: "Magento".to_string(),
            name: "backend".to_string(),
            area: Area::Adminhtml,
            path: temp.path().to_path_buf(),
            parent: None,
            theme_type: ThemeType::Luma,
        };
        let locale = LocaleCode::new("nl_NL");

        let path = output_path_for_theme(temp.path(), &theme, &locale);

        assert!(path.ends_with("pub/static/adminhtml/Magento/backend/nl_NL"));
    }

    // ==================== DeployJob tests ====================

    #[test]
    fn test_deploy_job_clone() {
        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/test"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme),
            locale: LocaleCode::new("en_US"),
        };

        let cloned = job.clone();
        assert_eq!(cloned.locale.as_str(), "en_US");
        assert_eq!(cloned.theme.vendor, "Test");
    }

    // ==================== job_matrix tests ====================

    #[test]
    fn test_job_matrix_single_theme_locale() {
        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/test"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let locales = vec![LocaleCode::new("en_US")];

        let jobs = job_matrix(&[theme], &locales);

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].theme.vendor, "Test");
        assert_eq!(jobs[0].locale.as_str(), "en_US");
    }

    #[test]
    fn test_job_matrix_multiple_themes_locales() {
        let theme1 = Theme {
            vendor: "A".to_string(),
            name: "one".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/a"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let theme2 = Theme {
            vendor: "B".to_string(),
            name: "two".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/b"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let locales = vec![LocaleCode::new("en_US"), LocaleCode::new("nl_NL")];

        let jobs = job_matrix(&[theme1, theme2], &locales);

        // 2 themes × 2 locales = 4 jobs
        assert_eq!(jobs.len(), 4);
    }

    #[test]
    fn test_job_matrix_empty_themes() {
        let locales = vec![LocaleCode::new("en_US")];
        let jobs = job_matrix(&[], &locales);
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_job_matrix_empty_locales() {
        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/test"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let jobs = job_matrix(&[theme], &[]);
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_job_matrix_arc_sharing() {
        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/test"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        let locales = vec![LocaleCode::new("en_US"), LocaleCode::new("nl_NL")];

        let jobs = job_matrix(&[theme], &locales);

        // Both jobs should share the same Arc
        assert!(Arc::ptr_eq(&jobs[0].theme, &jobs[1].theme));
    }

    // ==================== collect_results tests ====================

    fn make_result(status: DeployStatus) -> DeployResult {
        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/test"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        DeployResult {
            job: DeployJob {
                theme: Arc::new(theme),
                locale: LocaleCode::new("en_US"),
            },
            status,
            file_count: 0,
            duration: Duration::from_secs(1),
        }
    }

    #[test]
    fn test_collect_results_all_success() {
        let results = vec![
            make_result(DeployStatus::Success),
            make_result(DeployStatus::Success),
        ];

        let (collected, has_success, has_failure) = collect_results(results);

        assert_eq!(collected.len(), 2);
        assert!(has_success);
        assert!(!has_failure);
    }

    #[test]
    fn test_collect_results_all_failure() {
        let results = vec![
            make_result(DeployStatus::Failed(DeployError::Cancelled)),
            make_result(DeployStatus::Failed(DeployError::Cancelled)),
        ];

        let (_, has_success, has_failure) = collect_results(results);

        assert!(!has_success);
        assert!(has_failure);
    }

    #[test]
    fn test_collect_results_mixed() {
        let results = vec![
            make_result(DeployStatus::Success),
            make_result(DeployStatus::Failed(DeployError::Cancelled)),
            make_result(DeployStatus::Delegated),
        ];

        let (collected, has_success, has_failure) = collect_results(results);

        assert_eq!(collected.len(), 3);
        assert!(has_success);
        assert!(has_failure);
    }

    #[test]
    fn test_collect_results_delegated_counts_as_success() {
        let results = vec![make_result(DeployStatus::Delegated)];

        let (_, has_success, has_failure) = collect_results(results);

        assert!(has_success);
        assert!(!has_failure);
    }

    #[test]
    fn test_collect_results_cancelled_no_success_no_failure() {
        let results = vec![make_result(DeployStatus::Cancelled)];

        let (_, has_success, has_failure) = collect_results(results);

        assert!(!has_success);
        assert!(!has_failure);
    }

    #[test]
    fn test_collect_results_empty() {
        let results: Vec<DeployResult> = vec![];

        let (collected, has_success, has_failure) = collect_results(results);

        assert!(collected.is_empty());
        assert!(!has_success);
        assert!(!has_failure);
    }

    // ==================== DeployStatus tests ====================

    #[test]
    fn test_deploy_status_debug() {
        let status = DeployStatus::Success;
        let debug_str = format!("{:?}", status);
        assert_eq!(debug_str, "Success");
    }

    #[test]
    fn test_deploy_result_debug() {
        let result = make_result(DeployStatus::Success);
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("DeployResult"));
    }

    // ==================== read_deployed_version tests ====================

    #[test]
    fn test_read_deployed_version_exists() {
        let temp = TempDir::new().unwrap();
        let static_dir = temp.path().join("pub").join("static");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(static_dir.join("deployed_version.txt"), "12345\n").unwrap();

        let result = read_deployed_version(temp.path());
        assert_eq!(result, Some("12345".to_string()));
    }

    #[test]
    fn test_read_deployed_version_not_exists() {
        let temp = TempDir::new().unwrap();
        let result = read_deployed_version(temp.path());
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_deployed_version_trims_whitespace() {
        let temp = TempDir::new().unwrap();
        let static_dir = temp.path().join("pub").join("static");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(static_dir.join("deployed_version.txt"), "  version123  \n").unwrap();

        let result = read_deployed_version(temp.path());
        assert_eq!(result, Some("version123".to_string()));
    }

    // ==================== deploy_theme tests ====================

    #[test]
    fn test_deploy_theme_hyva_success() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let web_path = theme_path.join("web");
        std::fs::create_dir_all(&web_path).unwrap();
        std::fs::write(web_path.join("test.js"), "content").unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        let shutdown = AtomicBool::new(false);
        let stats = DeployStats::new();

        let result = deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert!(matches!(result.status, DeployStatus::Success));
        assert!(result.file_count > 0);
    }

    #[test]
    fn test_deploy_theme_cancelled_before_copy() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let web_path = theme_path.join("web");
        std::fs::create_dir_all(&web_path).unwrap();
        std::fs::write(web_path.join("test.js"), "content").unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        // Set shutdown before deploy
        let shutdown = AtomicBool::new(true);
        let stats = DeployStats::new();

        let result = deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert!(matches!(result.status, DeployStatus::Cancelled));
    }

    #[test]
    fn test_deploy_theme_no_sources() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        std::fs::create_dir_all(&theme_path).unwrap();
        // No web directory, no sources

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        let shutdown = AtomicBool::new(false);
        let stats = DeployStats::new();

        let result = deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert!(matches!(result.status, DeployStatus::Success));
        assert_eq!(result.file_count, 0);
    }

    #[test]
    fn test_deploy_theme_with_library() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        std::fs::create_dir_all(&theme_path).unwrap();

        // Create lib/web
        let lib_path = temp.path().join("lib").join("web");
        std::fs::create_dir_all(&lib_path).unwrap();
        std::fs::write(lib_path.join("lib.js"), "library").unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        let shutdown = AtomicBool::new(false);
        let stats = DeployStats::new();

        let result = deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert!(matches!(result.status, DeployStatus::Success));
        assert_eq!(result.file_count, 1);
    }

    #[test]
    fn test_deploy_theme_with_module_override() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let override_path = theme_path.join("Magento_Catalog").join("web");
        std::fs::create_dir_all(&override_path).unwrap();
        std::fs::write(override_path.join("catalog.js"), "override").unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        let shutdown = AtomicBool::new(false);
        let stats = DeployStats::new();

        let result = deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert!(matches!(result.status, DeployStatus::Success));
        assert_eq!(result.file_count, 1);

        // Check output path contains module folder
        let output = temp
            .path()
            .join("pub")
            .join("static")
            .join("frontend")
            .join("Test")
            .join("theme")
            .join("en_US")
            .join("Magento_Catalog")
            .join("catalog.js");
        assert!(output.exists());
    }

    #[test]
    fn test_deploy_theme_stats_updated() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let web_path = theme_path.join("web");
        std::fs::create_dir_all(&web_path).unwrap();
        std::fs::write(web_path.join("test.js"), "content123").unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let job = DeployJob {
            theme: Arc::new(theme.clone()),
            locale: LocaleCode::new("en_US"),
        };

        let shutdown = AtomicBool::new(false);
        let stats = DeployStats::new();

        deploy_theme(&job, &[theme], temp.path(), &shutdown, &stats, false, true);

        assert_eq!(stats.files_copied.0.load(Ordering::Relaxed), 1);
        assert!(stats.bytes_copied.0.load(Ordering::Relaxed) > 0);
        assert_eq!(stats.errors.0.load(Ordering::Relaxed), 0);
    }
}
