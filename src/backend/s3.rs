use async_trait::async_trait;
use bytes::Bytes;
use crate::error::Result;
use super::StorageBackend;

pub struct S3Backend {
    pub bucket: String,
    pub accel_prefix: String,
}

impl S3Backend {
    pub async fn new(
        _bucket: &str,
        _endpoint: &str,
        _region: &str,
        _accel_prefix: &str,
    ) -> Result<Self> {
        unimplemented!()
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn store(&self, _path: &str, _data: Bytes) -> Result<usize> {
        unimplemented!()
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("{}{}", self.accel_prefix, path)
    }
}
