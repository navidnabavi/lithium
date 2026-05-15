use async_trait::async_trait;
use aws_smithy_http::byte_stream::ByteStream;
use aws_types::region::Region;
use bytes::Bytes;

use crate::error::{LithiumError, Result};
use super::StorageBackend;

pub struct S3Backend {
    client: aws_sdk_s3::Client,
    bucket: String,
    pub accel_prefix: String,
}

impl S3Backend {
    pub async fn new(
        bucket: &str,
        endpoint: &str,
        region: &str,
        accel_prefix: &str,
    ) -> Result<Self> {
        let sdk_config = aws_config::from_env()
            .region(Region::new(region.to_string()))
            .endpoint_url(endpoint)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
            accel_prefix: accel_prefix.to_string(),
        })
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn store(&self, path: &str, data: Bytes) -> Result<usize> {
        let key = path.trim_start_matches('/').to_string();
        let data_len = data.len();
        let body = ByteStream::from(data);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .send()
            .await
            .map_err(|e| LithiumError::S3 { message: e.to_string() })?;

        Ok(data_len)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let key = path.trim_start_matches('/').to_string();

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| LithiumError::S3 { message: e.to_string() })?;

        Ok(())
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("{}{}", self.accel_prefix, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend() -> S3Backend {
        S3Backend {
            client: {
                // Build a minimal client — no network calls are made in this test.
                let config = aws_sdk_s3::config::Builder::new()
                    .region(Region::new("us-east-1"))
                    .build();
                aws_sdk_s3::Client::from_conf(config)
            },
            bucket: "test-bucket".to_string(),
            accel_prefix: "/s3".to_string(),
        }
    }

    #[test]
    fn test_accel_redirect_path() {
        let backend = make_backend();
        assert_eq!(backend.accel_redirect_path("/foo/bar.jpg"), "/s3/foo/bar.jpg");
    }
}
