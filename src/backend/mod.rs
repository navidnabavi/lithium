use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

use crate::config::BackendConfig;
use crate::error::Result;

pub mod file;
pub mod s3;

pub use file::FileBackend;
pub use s3::S3Backend;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store content at the given logical path. Returns number of bytes stored.
    async fn store(&self, path: &str, data: Bytes) -> Result<usize>;

    /// Delete content at the given logical path. Called by Sweeper on eviction.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Returns the nginx X-Accel-Redirect path for a cache hit on this backend.
    /// FileBackend returns "/files<path>", S3Backend returns "<accel_prefix><path>".
    fn accel_redirect_path(&self, path: &str) -> String;
}

/// Create a backend from config. Must be called inside a tokio runtime.
pub async fn create_backend(config: &BackendConfig) -> Result<Arc<dyn StorageBackend>> {
    match config {
        BackendConfig::File { base_dir } => {
            Ok(Arc::new(FileBackend::new(base_dir.clone())))
        }
        BackendConfig::S3 { bucket, endpoint, region, accel_prefix } => {
            let backend = S3Backend::new(bucket, endpoint, region, accel_prefix).await?;
            Ok(Arc::new(backend))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn test_mock_backend_store() {
        let backend = MockBackend;
        let data = Bytes::from("hello world");
        let size = backend.store("/test/file.jpg", data).await.unwrap();
        assert_eq!(size, 11);
    }

    #[tokio::test]
    async fn test_mock_backend_accel_redirect() {
        let backend = MockBackend;
        assert_eq!(backend.accel_redirect_path("/foo/bar.jpg"), "/mock/foo/bar.jpg");
    }
}
