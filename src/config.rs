use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub base_url: String,
    pub base_dir: PathBuf,
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
                size_limit: 100_000_000, // 100MB
                soft_limit_ratio: 0.85,
                sweep_interval_secs: 10,
                max_delete_per_iteration: 100,
                max_file_size: 10_000_000, // 10MB per file
            },
            base_url: "https://divar.ir".to_string(),
            base_dir: PathBuf::from("/tmp/lithium-cache"),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // Try to load from config file, fallback to default
        let config = if let Ok(config_str) = std::fs::read_to_string("lithium.toml") {
            toml::from_str(&config_str)?
        } else {
            Self::default()
        };
        
        // Validate configuration
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
        
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_str = toml::to_string_pretty(self)?;
        std::fs::write("lithium.toml", config_str)?;
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
        assert_eq!(config.cache.soft_limit_ratio, 0.85);
        assert_eq!(config.base_url, "https://divar.ir");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config.server.host, deserialized.server.host);
        assert_eq!(config.server.port, deserialized.server.port);
    }
}