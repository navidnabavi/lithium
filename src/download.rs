use crate::error::{LithiumError, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use tracing::info;
use url::Url;

pub async fn download_file(url: &str, filename: &str) -> Result<usize> {
    // Validate URL
    let parsed_url = Url::parse(url)?;
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(LithiumError::InvalidPath { 
            path: format!("Invalid URL scheme: {}", parsed_url.scheme()) 
        });
    }

    // Validate and clean the file path
    let path = Path::new(filename);

    // Reject paths containing parent directory components BEFORE cleaning
    // path_clean would resolve /../etc/shadow → /etc/shadow, bypassing the check
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(LithiumError::PathTraversal {
            path: filename.to_string(),
        });
    }

    let clean_path = PathBuf::from(path_clean::clean(path.to_str().unwrap_or("")));

    // Ensure the path is absolute
    if !clean_path.has_root() {
        return Err(LithiumError::InvalidPath {
            path: filename.to_string(),
        });
    }

    // Create parent directories
    if let Some(parent) = clean_path.parent() {
        fs::create_dir_all(parent)?;
    }

    info!("Downloading {} to {}", url, clean_path.display());

    // Create HTTP client
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Download the file
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(LithiumError::Download {
            message: format!("HTTP error: {}", response.status())
        });
    }

    let bytes = response.bytes().await?;
    let total_size = bytes.len();
    
    let mut file = fs::File::create(&clean_path)?;
    file.write_all(&bytes)?;

    file.sync_all()?;
    info!("Successfully downloaded {} bytes to {}", total_size, clean_path.display());

    Ok(total_size)
}

pub fn validate_path(path: &str, base_dir: &Path) -> Result<PathBuf> {
    let clean_path = PathBuf::from(path_clean::clean(path));
    
    // Check for path traversal attempts
    if clean_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(LithiumError::PathTraversal { 
            path: path.to_string() 
        });
    }

    let full_path = base_dir.join(clean_path);
    
    // Ensure the resolved path is still within the base directory
    if !full_path.starts_with(base_dir) {
        return Err(LithiumError::PathTraversal { 
            path: path.to_string() 
        });
    }

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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

    #[tokio::test]
    async fn test_download_file_rejects_traversal() {
        // /../etc/shadow — after path_clean becomes /etc/shadow (starts with "/", passes current check)
        // Must be rejected because raw path contains ParentDir components
        let result = download_file("https://example.com/file", "/../etc/shadow").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::LithiumError::PathTraversal { path } => {
                assert_eq!(path, "/../etc/shadow");
            }
            e => panic!("Expected PathTraversal error, got: {:?}", e),
        }
    }
}