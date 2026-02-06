//! # Magento Static Deploy
//!
//! High-performance static content deployment tool for Magento 2.
//!
//! This library provides fast deployment of static content for Hyva themes
//! by copying files directly rather than going through Magento's LESS compilation.
//!
//! ## Features
//!
//! - Parallel file copying using Rayon
//! - Theme inheritance chain resolution
//! - Module override support
//! - Development file filtering
//! - Progress tracking with atomic counters
//!
//! ## Usage
//!
//! ```ignore
//! use magento_static_deploy::scanner::discover_themes;
//! use magento_static_deploy::theme::Area;
//!
//! let themes = discover_themes(&magento_root, Area::Frontend)?;
//! ```

/// CLI configuration and argument parsing
pub mod config;

/// File copying operations with buffered I/O
pub mod copier;

/// Theme deployment orchestration
pub mod deployer;

/// Error types for deployment operations
pub mod error;

/// Theme and module scanning
pub mod scanner;

/// Theme, locale, and area types
pub mod theme;
