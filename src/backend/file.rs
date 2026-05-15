use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use crate::error::Result;
use super::StorageBackend;

pub struct FileBackend {
    pub base_dir: PathBuf,
}

impl FileBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

#[async_trait]
impl StorageBackend for FileBackend {
    async fn store(&self, _path: &str, _data: Bytes) -> Result<usize> {
        unimplemented!()
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("/files{}", path)
    }
}
