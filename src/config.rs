use clap::Parser;
use std::path::PathBuf;

use crate::theme::{Area, LocaleCode};

/// High-performance static content deployment for Magento 2
#[derive(Parser, Debug)]
#[command(name = "magento-static-deploy")]
#[command(version)]
#[command(about = "High-performance static content deployment for Magento 2")]
pub struct Cli {
    /// Magento root directory
    #[arg(default_value = ".")]
    pub magento_root: PathBuf,

    /// Areas to deploy (comma-separated)
    #[arg(
        short,
        long,
        value_delimiter = ',',
        default_value = "frontend,adminhtml"
    )]
    pub area: Vec<String>,

    /// Themes to deploy in Vendor/name format (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub theme: Option<Vec<String>>,

    /// Locales to deploy (comma-separated)
    #[arg(short, long, value_delimiter = ',', default_value = "en_US")]
    pub locale: Vec<String>,

    /// Number of parallel workers
    #[arg(short, long, default_value_t = num_cpus::get())]
    pub jobs: usize,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Include development files (.ts, .less, .md, node_modules, etc.)
    #[arg(short = 'd', long)]
    pub include_dev: bool,
}

/// Runtime configuration parsed from CLI
#[derive(Debug, Clone)]
pub struct Config {
    /// Magento root directory
    pub magento_root: PathBuf,
    /// Areas to deploy
    pub areas: Vec<Area>,
    /// Themes to deploy (None = all discovered)
    pub themes: Option<Vec<String>>,
    /// Locales to deploy (type-safe)
    pub locales: Vec<LocaleCode>,
    /// Number of parallel workers
    pub jobs: usize,
    /// Enable verbose output
    pub verbose: bool,
    /// Include development files (default: exclude)
    pub include_dev: bool,
}

impl Config {
    /// Create Config from CLI arguments
    pub fn from_cli(cli: Cli) -> anyhow::Result<Self> {
        let magento_root = cli.magento_root.canonicalize().unwrap_or(cli.magento_root);

        let areas = cli.area.iter().filter_map(|s| Area::parse(s)).collect();

        // Convert string locales to LocaleCode
        let locales = cli.locale.into_iter().map(LocaleCode::from).collect();

        Ok(Config {
            magento_root,
            areas,
            themes: cli.theme,
            locales,
            jobs: cli.jobs.max(1),
            verbose: cli.verbose,
            include_dev: cli.include_dev,
        })
    }
}
