use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendConfig {
    File {
        base_dir: PathBuf,
    },
    S3 {
        bucket: String,
        endpoint: String,
        region: String,
        accel_prefix: String,
    },
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig::File {
            base_dir: PathBuf::from("/tmp/lithium-cache"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub base_url: String,
    pub backend: BackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub size_limit: usize,
    pub soft_limit_ratio: f64,
    pub sweep_interval_secs: u64,
    pub max_delete_per_iteration: usize,
    pub max_file_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 9999,
            },
            cache: CacheConfig {
                size_limit: 100_000_000,
                soft_limit_ratio: 0.85,
                sweep_interval_secs: 10,
                max_delete_per_iteration: 100,
                max_file_size: 10_000_000,
            },
            base_url: "https://divar.ir".to_string(),
            backend: BackendConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config = if let Ok(config_str) = std::fs::read_to_string("lithium.toml") {
            toml::from_str(&config_str)?
        } else {
            Self::default()
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.cache.size_limit == 0 {
            return Err(anyhow::anyhow!("Cache size limit must be greater than 0"));
        }
        if self.cache.soft_limit_ratio <= 0.0 || self.cache.soft_limit_ratio >= 1.0 {
            return Err(anyhow::anyhow!("Soft limit ratio must be between 0 and 1"));
        }
        if self.cache.sweep_interval_secs == 0 {
            return Err(anyhow::anyhow!("Sweep interval must be greater than 0"));
        }
        if self.cache.max_delete_per_iteration == 0 {
            return Err(anyhow::anyhow!("Max delete per iteration must be greater than 0"));
        }
        if self.cache.max_file_size == 0 {
            return Err(anyhow::anyhow!("Max file size must be greater than 0"));
        }
        if self.cache.max_file_size > self.cache.size_limit {
            return Err(anyhow::anyhow!("Max file size cannot be larger than cache size limit"));
        }
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("Port must be greater than 0"));
        }
        if self.base_url.is_empty() {
            return Err(anyhow::anyhow!("Base URL cannot be empty"));
        }
        if let BackendConfig::S3 { accel_prefix, .. } = &self.backend {
            if accel_prefix.is_empty() {
                return Err(anyhow::anyhow!("S3 accel_prefix cannot be empty"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9999);
        assert_eq!(config.cache.size_limit, 100_000_000);
        assert_eq!(config.base_url, "https://divar.ir");
        assert!(matches!(config.backend, BackendConfig::File { .. }));
    }

    #[test]
    fn test_backend_config_file_deserialization() {
        let toml_str = r#"
            type = "file"
            base_dir = "/tmp/cache"
        "#;
        let backend: BackendConfig = toml::from_str(toml_str).unwrap();
        match backend {
            BackendConfig::File { base_dir } => {
                assert_eq!(base_dir, std::path::PathBuf::from("/tmp/cache"));
            }
            _ => panic!("Expected File backend"),
        }
    }

    #[test]
    fn test_backend_config_s3_deserialization() {
        let toml_str = r#"
            type = "s3"
            bucket = "my-bucket"
            endpoint = "https://s3.example.com"
            region = "us-east-1"
            accel_prefix = "/s3-internal"
        "#;
        let backend: BackendConfig = toml::from_str(toml_str).unwrap();
        match backend {
            BackendConfig::S3 { bucket, endpoint, region, accel_prefix } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(endpoint, "https://s3.example.com");
                assert_eq!(region, "us-east-1");
                assert_eq!(accel_prefix, "/s3-internal");
            }
            _ => panic!("Expected S3 backend"),
        }
    }
}
