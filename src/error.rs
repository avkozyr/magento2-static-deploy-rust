//! Error types for static content deployment operations.

use std::path::PathBuf;
use thiserror::Error;

/// Deployment error types with detailed context for debugging.
///
/// All errors include relevant paths and operation context to help
/// diagnose deployment failures quickly.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum DeployError {
    #[error("Magento root not found: {path}")]
    RootNotFound { path: PathBuf },

    #[error("No space left on device for {path}")]
    DiskFull { path: PathBuf },

    #[error("Theme not found: {theme}")]
    ThemeNotFound { theme: String },

    #[error("Invalid theme.xml: {path}")]
    InvalidThemeXml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },

    #[error("Failed to copy {src} to {dst}")]
    CopyFailed {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create directory: {path}")]
    CreateDirFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("bin/magento setup:static-content:deploy failed with exit code {code}: {stderr}")]
    MagentoFailed { code: i32, stderr: String },

    #[error("Invalid locale format '{locale}': expected xx_YY (e.g., en_US)")]
    InvalidLocale { locale: String },

    #[error("Deployment cancelled")]
    Cancelled,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_root_not_found_display() {
        let err = DeployError::RootNotFound {
            path: PathBuf::from("/nonexistent"),
        };
        assert!(format!("{}", err).contains("Magento root not found"));
    }

    #[test]
    fn test_error_disk_full_display() {
        let err = DeployError::DiskFull {
            path: PathBuf::from("/disk"),
        };
        assert!(format!("{}", err).contains("No space left on device"));
    }

    #[test]
    fn test_error_theme_not_found_display() {
        let err = DeployError::ThemeNotFound {
            theme: "Test/theme".to_string(),
        };
        assert!(format!("{}", err).contains("Theme not found"));
    }

    #[test]
    fn test_error_copy_failed_display() {
        let err = DeployError::CopyFailed {
            src: PathBuf::from("/src"),
            dst: PathBuf::from("/dst"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert!(format!("{}", err).contains("Failed to copy"));
    }

    #[test]
    fn test_error_create_dir_failed_display() {
        let err = DeployError::CreateDirFailed {
            path: PathBuf::from("/dir"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        assert!(format!("{}", err).contains("Failed to create directory"));
    }

    #[test]
    fn test_error_magento_failed_display() {
        let err = DeployError::MagentoFailed {
            code: 1,
            stderr: "error".to_string(),
        };
        assert!(format!("{}", err).contains("bin/magento"));
    }

    #[test]
    fn test_error_invalid_locale_display() {
        let err = DeployError::InvalidLocale {
            locale: "invalid".to_string(),
        };
        assert!(format!("{}", err).contains("Invalid locale format"));
    }

    #[test]
    fn test_error_cancelled_display() {
        let err = DeployError::Cancelled;
        assert!(format!("{}", err).contains("cancelled"));
    }

    #[test]
    fn test_error_io_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "io error");
        let err = DeployError::Io(io_err);
        assert!(format!("{}", err).contains("IO error"));
    }

    #[test]
    fn test_error_debug() {
        let err = DeployError::Cancelled;
        let debug = format!("{:?}", err);
        assert!(debug.contains("Cancelled"));
    }

    #[test]
    fn test_error_io_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let err: DeployError = io_err.into();
        assert!(matches!(err, DeployError::Io(_)));
    }
}
