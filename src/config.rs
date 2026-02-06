//! CLI configuration and runtime settings for static content deployment.

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

        // Validate and convert string locales to LocaleCode (FR-010)
        let mut locales = Vec::with_capacity(cli.locale.len());
        for locale_str in cli.locale {
            match LocaleCode::validated(&locale_str) {
                Ok(locale) => locales.push(locale),
                Err(msg) => anyhow::bail!(msg),
            }
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cli(
        area: Vec<String>,
        locale: Vec<String>,
        theme: Option<Vec<String>>,
        jobs: usize,
        verbose: bool,
        include_dev: bool,
    ) -> Cli {
        Cli {
            magento_root: PathBuf::from("/tmp"),
            area,
            theme,
            locale,
            jobs,
            verbose,
            include_dev,
        }
    }

    // ==================== Cli defaults tests ====================

    #[test]
    fn test_cli_debug() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            false,
        );
        let debug = format!("{:?}", cli);
        assert!(debug.contains("Cli"));
    }

    // ==================== Config::from_cli tests ====================

    #[test]
    fn test_config_from_cli_basic() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert_eq!(config.areas, vec![Area::Frontend]);
        assert_eq!(config.locales.len(), 1);
        assert_eq!(config.locales[0].as_str(), "en_US");
        assert_eq!(config.jobs, 4);
        assert!(!config.verbose);
        assert!(!config.include_dev);
    }

    #[test]
    fn test_config_from_cli_multiple_areas() {
        let cli = make_cli(
            vec!["frontend".to_string(), "adminhtml".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert_eq!(config.areas, vec![Area::Frontend, Area::Adminhtml]);
    }

    #[test]
    fn test_config_from_cli_multiple_locales() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec![
                "en_US".to_string(),
                "nl_NL".to_string(),
                "de_DE".to_string(),
            ],
            None,
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert_eq!(config.locales.len(), 3);
        assert_eq!(config.locales[0].as_str(), "en_US");
        assert_eq!(config.locales[1].as_str(), "nl_NL");
        assert_eq!(config.locales[2].as_str(), "de_DE");
    }

    #[test]
    fn test_config_from_cli_with_themes() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            Some(vec![
                "Hyva/default".to_string(),
                "Magento/blank".to_string(),
            ]),
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert!(config.themes.is_some());
        let themes = config.themes.unwrap();
        assert_eq!(themes.len(), 2);
        assert_eq!(themes[0], "Hyva/default");
        assert_eq!(themes[1], "Magento/blank");
    }

    #[test]
    fn test_config_from_cli_verbose() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            true,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert!(config.verbose);
    }

    #[test]
    fn test_config_from_cli_include_dev() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            true,
        );

        let config = Config::from_cli(cli).unwrap();

        assert!(config.include_dev);
    }

    #[test]
    fn test_config_from_cli_jobs_minimum_one() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            0, // Zero jobs should become 1
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        assert_eq!(config.jobs, 1);
    }

    #[test]
    fn test_config_from_cli_invalid_locale_format() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["invalid".to_string()],
            None,
            4,
            false,
            false,
        );

        let result = Config::from_cli(cli);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_from_cli_invalid_area_ignored() {
        let cli = make_cli(
            vec!["invalid".to_string(), "frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();

        // Invalid area is filtered out, only frontend remains
        assert_eq!(config.areas, vec![Area::Frontend]);
    }

    #[test]
    fn test_config_from_cli_lowercase_locale() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_us".to_string()], // lowercase
            None,
            4,
            false,
            false,
        );

        let result = Config::from_cli(cli);
        // Lowercase should fail validation (requires en_US format)
        assert!(result.is_err());
    }

    // ==================== Config Clone tests ====================

    #[test]
    fn test_config_clone() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            Some(vec!["Hyva/default".to_string()]),
            8,
            true,
            true,
        );

        let config = Config::from_cli(cli).unwrap();
        let cloned = config.clone();

        assert_eq!(config.areas, cloned.areas);
        assert_eq!(config.locales, cloned.locales);
        assert_eq!(config.jobs, cloned.jobs);
        assert_eq!(config.verbose, cloned.verbose);
        assert_eq!(config.include_dev, cloned.include_dev);
    }

    #[test]
    fn test_config_debug() {
        let cli = make_cli(
            vec!["frontend".to_string()],
            vec!["en_US".to_string()],
            None,
            4,
            false,
            false,
        );

        let config = Config::from_cli(cli).unwrap();
        let debug = format!("{:?}", config);

        assert!(debug.contains("Config"));
        assert!(debug.contains("Frontend"));
    }
}
