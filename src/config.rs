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
    pub sweeper: SweeperConfig,
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
    pub max_file_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweeperConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_size_limit")]
    pub size_limit: usize,
    #[serde(default = "default_soft_limit_ratio")]
    pub soft_limit_ratio: f64,
    #[serde(default = "default_sweep_interval_secs")]
    pub sweep_interval_secs: u64,
    #[serde(default = "default_max_delete_per_iteration")]
    pub max_delete_per_iteration: usize,
}

fn default_true() -> bool { true }
fn default_size_limit() -> usize { 100_000_000 }
fn default_soft_limit_ratio() -> f64 { 0.85 }
fn default_sweep_interval_secs() -> u64 { 10 }
fn default_max_delete_per_iteration() -> usize { 100 }

impl Default for SweeperConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size_limit: default_size_limit(),
            soft_limit_ratio: default_soft_limit_ratio(),
            sweep_interval_secs: default_sweep_interval_secs(),
            max_delete_per_iteration: default_max_delete_per_iteration(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 9999,
            },
            cache: CacheConfig {
                max_file_size: 10_000_000,
            },
            sweeper: SweeperConfig::default(),
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
        if self.cache.max_file_size == 0 {
            return Err(anyhow::anyhow!("Max file size must be greater than 0"));
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
        if self.sweeper.enabled {
            if self.sweeper.size_limit == 0 {
                return Err(anyhow::anyhow!("Sweeper size limit must be greater than 0"));
            }
            if self.sweeper.soft_limit_ratio <= 0.0 || self.sweeper.soft_limit_ratio >= 1.0 {
                return Err(anyhow::anyhow!("Soft limit ratio must be between 0 and 1"));
            }
            if self.sweeper.sweep_interval_secs == 0 {
                return Err(anyhow::anyhow!("Sweep interval must be greater than 0"));
            }
            if self.sweeper.max_delete_per_iteration == 0 {
                return Err(anyhow::anyhow!("Max delete per iteration must be greater than 0"));
            }
            if self.cache.max_file_size > self.sweeper.size_limit {
                return Err(anyhow::anyhow!("Max file size cannot be larger than sweeper size limit"));
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
        assert_eq!(config.sweeper.size_limit, 100_000_000);
        assert_eq!(config.base_url, "https://divar.ir");
        assert!(matches!(config.backend, BackendConfig::File { .. }));
    }

    #[test]
    fn test_sweeper_config_deserialization() {
        let toml_str = r#"
            enabled = true
            size_limit = 50_000_000
            soft_limit_ratio = 0.9
            sweep_interval_secs = 5
            max_delete_per_iteration = 50
        "#;
        let sweeper: SweeperConfig = toml::from_str(toml_str).unwrap();
        assert!(sweeper.enabled);
        assert_eq!(sweeper.size_limit, 50_000_000);
        assert_eq!(sweeper.soft_limit_ratio, 0.9);
        assert_eq!(sweeper.sweep_interval_secs, 5);
        assert_eq!(sweeper.max_delete_per_iteration, 50);
    }

    #[test]
    fn test_sweeper_config_disabled_uses_defaults() {
        let toml_str = r#"enabled = false"#;
        let sweeper: SweeperConfig = toml::from_str(toml_str).unwrap();
        assert!(!sweeper.enabled);
        let _ = sweeper.size_limit;
    }

    #[test]
    fn test_cache_config_only_has_max_file_size() {
        let toml_str = r#"max_file_size = 5_000_000"#;
        let cache: CacheConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cache.max_file_size, 5_000_000);
    }

    #[test]
    fn test_full_config_with_sweeper_section() {
        let toml_str = r#"
            base_url = "https://example.com"

            [server]
            host = "127.0.0.1"
            port = 8080

            [cache]
            max_file_size = 10_000_000

            [sweeper]
            enabled = true
            size_limit = 100_000_000
            soft_limit_ratio = 0.85
            sweep_interval_secs = 10
            max_delete_per_iteration = 100

            [backend]
            type = "file"
            base_dir = "/tmp/cache"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.cache.max_file_size, 10_000_000);
        assert!(config.sweeper.enabled);
        assert_eq!(config.sweeper.size_limit, 100_000_000);
    }

    #[test]
    fn test_validation_skips_sweep_fields_when_disabled() {
        let mut config = Config::default();
        config.sweeper.enabled = false;
        config.sweeper.size_limit = 0;
        config.sweeper.soft_limit_ratio = 0.0;
        config.sweeper.sweep_interval_secs = 0;
        config.sweeper.max_delete_per_iteration = 0;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_zero_size_limit_when_enabled() {
        let mut config = Config::default();
        config.sweeper.enabled = true;
        config.sweeper.size_limit = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_bad_soft_limit_ratio_when_enabled() {
        let mut config = Config::default();
        config.sweeper.enabled = true;
        config.sweeper.soft_limit_ratio = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_max_file_size_exceeding_size_limit_when_enabled() {
        let mut config = Config::default();
        config.sweeper.enabled = true;
        config.sweeper.size_limit = 100;
        config.cache.max_file_size = 200;
        assert!(config.validate().is_err());
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
