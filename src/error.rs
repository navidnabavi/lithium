use thiserror::Error;

#[derive(Error, Debug)]
pub enum LithiumError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Configuration error: {0}")]
    Config(#[from] toml::de::Error),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Path traversal detected: {path}")]
    PathTraversal { path: String },

    #[error("Cache error: {message}")]
    Cache { message: String },

    #[error("Download error: {message}")]
    Download { message: String },

    #[error("Invalid path: {path}")]
    InvalidPath { path: String },

    #[error("S3 error: {message}")]
    S3 { message: String },
}

pub type Result<T> = std::result::Result<T, LithiumError>;
