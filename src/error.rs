use std::path::PathBuf;
use thiserror::Error;

/// Deployment error types
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

    #[error("Deployment cancelled")]
    Cancelled,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
