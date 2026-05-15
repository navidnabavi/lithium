use bytes::Bytes;
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

    // Reject paths containing parent directory components
    // This is backend-agnostic security: must happen before any backend call
    if std::path::Path::new(path)
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

// validate_path remains for callers that need base-dir-scoped path validation
pub fn validate_path(path: &str, base_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    let clean_path = std::path::PathBuf::from(path_clean::clean(path));

    if clean_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    let full_path = base_dir.join(clean_path);

    if !full_path.starts_with(base_dir) {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use async_trait::async_trait;

    // ---- validate_path tests (unchanged) ----

    #[test]
    fn test_validate_path_safe() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("safe/path/file.txt", base_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/tmp/cache/safe/path/file.txt"));
    }

    #[test]
    fn test_validate_path_traversal() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("../../../etc/passwd", base_dir);
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::PathTraversal { path } => {
                assert_eq!(path, "../../../etc/passwd");
            }
            _ => panic!("Expected PathTraversal error"),
        }
    }

    #[test]
    fn test_validate_path_absolute() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("absolute/path", base_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/tmp/cache/absolute/path"));
    }

    // ---- download_file tests ----

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
        let result = download_file(&client, &backend, "https://example.com/file", "/../etc/shadow").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::PathTraversal { path } => {
                assert_eq!(path, "/../etc/shadow");
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
