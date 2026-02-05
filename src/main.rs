use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use magento_static_deploy::config::{Cli, Config};
use magento_static_deploy::deployer::{
    collect_results, deploy_theme, job_matrix, DeployStats, DeployStatus,
};
use magento_static_deploy::scanner::discover_themes;
use magento_static_deploy::theme::Theme;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let config = Config::from_cli(cli)?;

    // Validate Magento root
    if !config.magento_root.exists() {
        bail!("Magento root not found: {}", config.magento_root.display());
    }

    let env_php = config.magento_root.join("app").join("etc").join("env.php");
    if !env_php.exists() {
        bail!(
            "Not a Magento installation: {} (app/etc/env.php not found)",
            config.magento_root.display()
        );
    }

    // Setup Ctrl+C handler
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")?;

    // Configure Rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(config.jobs)
        .build_global()
        .ok();

    // Discover themes
    let mut all_themes: Vec<Theme> = Vec::new();
    for area in &config.areas {
        let themes = discover_themes(&config.magento_root, *area)
            .with_context(|| format!("Failed to discover themes in {}", area.as_str()))?;
        all_themes.extend(themes);
    }

    if all_themes.is_empty() {
        bail!("No themes found in {}", config.magento_root.display());
    }

    // Filter themes if specified
    let deploy_themes: Vec<&Theme> = if let Some(ref theme_filters) = config.themes {
        all_themes
            .iter()
            .filter(|t| theme_filters.contains(&t.full_name()))
            .collect()
    } else {
        all_themes.iter().collect()
    };

    if deploy_themes.is_empty() {
        if let Some(ref filters) = config.themes {
            bail!("No matching themes found for: {}", filters.join(", "));
        }
    }

    // Generate job matrix
    let jobs = job_matrix(
        &deploy_themes.iter().cloned().cloned().collect::<Vec<_>>(),
        &config.locales,
    );

    let total_jobs = jobs.len();
    if config.verbose {
        eprintln!(
            "Deploying {} theme(s) × {} locale(s) = {} job(s) with {} worker(s)",
            deploy_themes.len(),
            config.locales.len(),
            total_jobs,
            config.jobs
        );
    }

    let start = Instant::now();
    let stats = DeployStats::new();

    // Setup progress bars (only in verbose mode)
    let multi_progress = MultiProgress::new();
    let main_progress = if config.verbose {
        let pb = multi_progress.add(ProgressBar::new(total_jobs as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("#>-"),
        );
        pb.set_message("Deploying themes...");
        Some(pb)
    } else {
        None
    };

    // Execute jobs in parallel
    let results: Vec<_> = jobs
        .par_iter()
        .enumerate()
        .map(|(idx, job)| {
            if config.verbose {
                eprintln!(
                    "[{}/{}] Deploying {}/{}/{}...",
                    idx + 1,
                    total_jobs,
                    job.theme.area.as_str(),
                    job.theme.full_name(),
                    job.locale
                );
            }

            let result = deploy_theme(
                job,
                &all_themes,
                &config.magento_root,
                &shutdown,
                &stats,
                config.verbose,
                config.include_dev,
            );

            // Update progress bar
            if let Some(ref pb) = main_progress {
                pb.inc(1);
            }

            result
        })
        .collect();

    // Finish progress bar
    if let Some(pb) = main_progress {
        pb.finish_with_message("Complete");
    }

    // Check for cancellation
    if shutdown.load(Ordering::Relaxed) {
        eprintln!("\nDeployment cancelled");
        return Ok(ExitCode::from(130));
    }

    // Aggregate results
    let (results, has_success, has_failure) = collect_results(results);
    let duration = start.elapsed();
    let total_files = stats.files_copied.0.load(Ordering::Relaxed);
    let throughput = if duration.as_secs_f64() > 0.0 {
        total_files as f64 / duration.as_secs_f64()
    } else {
        0.0
    };

    // Print summary
    println!(
        "Deployed {} files in {:.2}s ({:.0} files/sec)",
        total_files,
        duration.as_secs_f64(),
        throughput
    );

    // Per-job breakdown
    for result in &results {
        let status_str = match &result.status {
            DeployStatus::Success => format!("{} files", result.file_count),
            DeployStatus::Delegated => "delegated to bin/magento".to_string(),
            DeployStatus::Failed(e) => format!("FAILED: {e}"),
            DeployStatus::Cancelled => "cancelled".to_string(),
        };

        println!(
            "  {}/{}/{}: {}",
            result.job.theme.area.as_str(),
            result.job.theme.full_name(),
            result.job.locale,
            status_str
        );
    }

    // Determine exit code
    if has_failure && !has_success {
        Ok(ExitCode::from(2))
    } else if has_failure {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}
