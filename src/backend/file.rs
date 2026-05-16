use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use tracing::{error, info};

use super::StorageBackend;
use crate::error::{LithiumError, Result};

pub struct FileBackend {
    pub base_dir: PathBuf,
}

impl FileBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        // path is like "/some/file.jpg" — strip leading slash before joining
        let stripped = path.trim_start_matches('/');
        self.base_dir.join(stripped)
    }
}

#[async_trait]
impl StorageBackend for FileBackend {
    async fn store(&self, path: &str, data: Bytes) -> Result<usize> {
        let full_path = self.full_path(path);

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let size = data.len();
        tokio::fs::write(&full_path, data).await?;
        info!(
            "FileBackend: stored {} bytes at {}",
            size,
            full_path.display()
        );
        Ok(size)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        tokio::fs::remove_file(&full_path).await.map_err(|e| {
            error!(
                "FileBackend: failed to delete {}: {}",
                full_path.display(),
                e
            );
            LithiumError::Io(e)
        })?;
        info!("FileBackend: deleted {}", full_path.display());
        Ok(())
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("/files{}", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_store_creates_file() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());
        let data = Bytes::from("hello lithium");

        let size = backend.store("/subdir/test.txt", data).await.unwrap();

        assert_eq!(size, 13);
        let stored = tokio::fs::read(dir.path().join("subdir/test.txt"))
            .await
            .unwrap();
        assert_eq!(stored, b"hello lithium");
    }

    #[tokio::test]
    async fn test_store_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        backend
            .store("/a/b/c/file.bin", Bytes::from("x"))
            .await
            .unwrap();

        assert!(dir.path().join("a/b/c/file.bin").exists());
    }

    #[tokio::test]
    async fn test_delete_removes_file() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        backend
            .store("/to_delete.txt", Bytes::from("data"))
            .await
            .unwrap();
        assert!(dir.path().join("to_delete.txt").exists());

        backend.delete("/to_delete.txt").await.unwrap();
        assert!(!dir.path().join("to_delete.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_missing_file_returns_error() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        let result = backend.delete("/nonexistent.txt").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LithiumError::Io(_)));
    }

    #[test]
    fn test_accel_redirect_path() {
        let backend = FileBackend::new(PathBuf::from("/tmp/cache"));
        assert_eq!(
            backend.accel_redirect_path("/images/cat.jpg"),
            "/files/images/cat.jpg"
        );
    }
}
