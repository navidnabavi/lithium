use bytes::Bytes;
use path_clean::PathClean;
use std::path::PathBuf;
use tracing::info;
use url::Url;

use crate::backend::StorageBackend;
use crate::error::{LithiumError, Result};

pub async fn download_file(
    client: &reqwest::Client,
    backend: &dyn StorageBackend,
    url: &str,
    path: &str,
) -> Result<usize> {
    // Validate URL scheme
    let parsed_url = Url::parse(url)?;
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(LithiumError::InvalidPath {
            path: format!("Invalid URL scheme: {}", parsed_url.scheme()),
        });
    }

    // Normalize path first, then reject paths containing parent directory components
    // This is backend-agnostic security: must happen before any backend call
    let normalized = PathBuf::from(path).clean();
    if normalized
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    info!("Downloading {} → backend path {}", url, path);

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(LithiumError::Download {
            message: format!("HTTP error: {}", response.status()),
        });
    }

    let bytes: Bytes = response.bytes().await?;
    let size = backend.store(path, bytes).await?;

    info!("Downloaded and stored {} bytes at {}", size, path);
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockBackend;

    #[async_trait]
    impl StorageBackend for MockBackend {
        async fn store(&self, _path: &str, data: Bytes) -> Result<usize> {
            Ok(data.len())
        }
        async fn delete(&self, _path: &str) -> Result<()> {
            Ok(())
        }
        fn accel_redirect_path(&self, path: &str) -> String {
            format!("/mock{}", path)
        }
    }

    #[tokio::test]
    async fn test_download_file_rejects_traversal() {
        let client = reqwest::Client::new();
        let backend = MockBackend;
        let result = download_file(&client, &backend, "https://example.com/file", "../etc/shadow").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::PathTraversal { path } => {
                assert_eq!(path, "../etc/shadow");
            }
            e => panic!("Expected PathTraversal, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_download_file_rejects_invalid_scheme() {
        let client = reqwest::Client::new();
        let backend = MockBackend;
        let result = download_file(&client, &backend, "ftp://example.com/file", "/valid/path").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LithiumError::InvalidPath { .. }));
    }
}
