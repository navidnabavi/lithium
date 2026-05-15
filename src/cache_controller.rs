use crate::error::{LithiumError, Result};
use std::sync::{Arc, RwLock, Mutex};
use std::collections::{HashMap, BTreeMap, HashSet};
use std::thread::{JoinHandle, spawn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::mpsc::{Sender, channel};
use std::fs;
use std::path::PathBuf;
use tracing::{info, error, debug};

#[derive(Debug, Clone)]
struct TimeUrl {
    url: String,
    time: u64, // unix timestamp in nanoseconds
    size: usize,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

impl TimeUrl {
    fn new(url: String) -> Self {
        Self {
            url: url.to_owned(),
            time: now(),
            size: 0,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum HitMiss {
    Hit,
    Miss,
    Downloading,
}

pub struct CacheController {
    urls: BTreeMap<(u64, String), ()>,
    url_map: HashMap<String, TimeUrl>,
    downloading: Arc<Mutex<HashSet<String>>>, // Track URLs currently being downloaded
    size: usize,
    size_limit: usize,
    soft_limit_ratio: f64,
    sweep_interval_secs: u64,
    max_delete_per_iteration: usize,
    max_file_size: usize,
}

impl CacheController {
    pub fn new(size_limit: usize, soft_limit_ratio: f64, sweep_interval_secs: u64, max_delete_per_iteration: usize, max_file_size: usize) -> Self {
        Self {
            urls: BTreeMap::new(),
            url_map: HashMap::new(),
            downloading: Arc::new(Mutex::new(HashSet::new())),
            size: 0,
            size_limit,
            soft_limit_ratio,
            sweep_interval_secs,
            max_delete_per_iteration,
            max_file_size,
        }
    }

    pub fn access(&mut self, url: &str) -> HitMiss {
        // Check if already in cache
        if self.url_map.get(url).is_some() {
            return self.update(url);
        }
        
        // Check if currently downloading
        if let Ok(downloading) = self.downloading.lock() {
            if downloading.contains(url) {
                return HitMiss::Downloading;
            }
        }
        
        // Mark as downloading and add to cache
        if let Ok(mut downloading) = self.downloading.lock() {
            downloading.insert(url.to_string());
        }
        self.push(url);
        HitMiss::Miss
    }

    fn push(&mut self, url: &str) {
        let data = TimeUrl::new(url.to_owned());
        self.urls.insert((data.time, url.to_owned()), ());
        self.url_map.insert(url.to_owned(), data);
        debug!("Added {} to cache", url);
    }

    fn update(&mut self, url: &str) -> HitMiss {
        if let Some(v) = self.url_map.get_mut(url) {
            match v.size {
                0 => HitMiss::Downloading,
                _ => {
                    self.urls.remove(&(v.time, v.url.clone()));
                    v.time = now();
                    self.urls.insert((v.time, v.url.clone()), ());
                    HitMiss::Hit
                }
            }
        } else {
            error!("Cache inconsistency: URL {} not found in url_map", url);
            HitMiss::Miss
        }
    }

    pub fn remove(&mut self, url: &str) {
        if let Some(time_url) = self.url_map.remove(url) {
            self.urls.remove(&(time_url.time, url.to_owned()));
            self.size = self.size.saturating_sub(time_url.size);
            debug!("Removed {} from cache", url);
        }
        
        // Also remove from downloading set
        if let Ok(mut downloading) = self.downloading.lock() {
            downloading.remove(url);
        }
    }
    
    pub fn download_failed(&mut self, url: &str) {
        // Remove from downloading set
        if let Ok(mut downloading) = self.downloading.lock() {
            downloading.remove(url);
        }
        
        // Remove from cache if it exists
        self.remove(url);
        error!("Download failed for {}", url);
    }

    pub fn download_done(&mut self, url: &str, size: usize) -> Result<()> {
        // Check file size limit
        if size > self.max_file_size {
            // Remove from downloading set
            if let Ok(mut downloading) = self.downloading.lock() {
                downloading.remove(url);
            }
            return Err(LithiumError::Download {
                message: format!("File too large: {} bytes (max: {})", size, self.max_file_size)
            });
        }
        
        if let Some(v) = self.url_map.get_mut(url) {
            v.size = size;
            self.size += size;
            
            // Remove from downloading set
            if let Ok(mut downloading) = self.downloading.lock() {
                downloading.remove(url);
            }
            
            info!("Download completed for {}: {} bytes", url, size);
            Ok(())
        } else {
            // Remove from downloading set on error
            if let Ok(mut downloading) = self.downloading.lock() {
                downloading.remove(url);
            }
            Err(LithiumError::Cache {
                message: format!("URL {} not found in cache", url)
            })
        }
    }

    pub fn set_size_limit(&mut self, limit: usize) {
        self.size_limit = limit;
        info!("Cache size limit set to {} bytes", limit);
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.url_map.len(), self.size)
    }

    pub fn dump(&self) {
        info!("Cache stats: {} entries, {} bytes", self.url_map.len(), self.size);
        for (url, time_url) in &self.url_map {
            debug!("  {}: {} bytes, accessed at {}", url, time_url.size, time_url.time);
        }
    }

    fn sweep(&mut self, file_deleter: &Sender<String>) -> u64 {
        let max_delete = self.max_delete_per_iteration;
        let mut deleted = 0;
        
        while deleted < max_delete && self.soft_limit_passed() {
            if self.sweep_once(file_deleter) {
                deleted += 1;
            } else {
                break; // No more items to delete
            }
        }
        
        if deleted > 0 {
            info!("Swept {} items from cache", deleted);
        }
        
        self.sweep_interval_secs
    }

    fn get_oldest(&self) -> Option<(u64, String)> {
        self.urls.iter().next().map(|((time, url), _)| (*time, url.clone()))
    }

    fn sweep_once(&mut self, file_deleter: &Sender<String>) -> bool {
        if let Some((time, url)) = self.get_oldest() {
            if let Some(time_url) = self.url_map.get(&url) {
                let size = time_url.size;

                if let Err(e) = file_deleter.send(url.clone()) {
                    error!("Failed to send file deletion request: {}", e);
                    return false;
                }

                // Only update accounting after successful send
                self.size = self.size.saturating_sub(size);
                self.url_map.remove(&url);
                self.urls.remove(&(time, url.clone()));
                info!("Scheduled removal of {}", url);
                true
            } else {
                error!("Cache inconsistency: URL {} not found in url_map", url);
                false
            }
        } else {
            false
        }
    }

    pub fn soft_limit_passed(&self) -> bool {
        self.size > (self.size_limit as f64 * self.soft_limit_ratio) as usize
    }
    
    pub fn hard_limit_passed(&self) -> bool {
        self.size > self.size_limit
    }

}

pub struct Sweeper {
    sweeper_handle: JoinHandle<()>,
    file_deleter_handle: JoinHandle<()>,
}

impl Sweeper {
    pub fn new(cache_controller: Arc<RwLock<CacheController>>, base_dir: PathBuf) -> Self {
        let (tx, rx) = channel();

        let sweeper_handle = spawn(move || {
            info!("Sweeper thread started");
            let delete_channel = tx;
            loop {
                let delay = {
                    match cache_controller.write() {
                        Ok(mut cache) => cache.sweep(&delete_channel),
                        Err(e) => {
                            error!("Failed to acquire cache lock in sweeper: {}", e);
                            60 // Wait 1 minute before retrying
                        }
                    }
                };
                std::thread::sleep(Duration::from_secs(delay));
            }
        });

        let file_deleter_handle = spawn(move || {
            info!("File deleter thread started");
            while let Ok(message) = rx.recv() {
                let file_path = base_dir.join(&message);
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete file {}: {}", file_path.display(), e);
                } else {
                    info!("Deleted file: {}", file_path.display());
                }
            }
        });

        Self {
            sweeper_handle,
            file_deleter_handle,
        }
    }

    pub fn join(self) {
        let _ = self.sweeper_handle.join();
        let _ = self.file_deleter_handle.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache() {
        let mut cache_controller = CacheController::new(1000, 0.8, 1, 10, 100);
        
        // Test initial access (should be miss)
        assert_eq!(cache_controller.access("hello"), HitMiss::Miss);
        assert_eq!(cache_controller.access("hello2"), HitMiss::Miss);
        
        // Test downloading state
        assert_eq!(cache_controller.access("hello"), HitMiss::Downloading);
        
        // Test download completion
        assert!(cache_controller.download_done("hello", 10).is_ok());
        
        // Test hit after download
        assert_eq!(cache_controller.access("hello"), HitMiss::Hit);
        
        // Test stats
        let (entries, size) = cache_controller.stats();
        assert_eq!(entries, 2);
        assert_eq!(size, 10);
    }
    
    #[test]
    fn test_sweep_size_accounting_on_send_failure() {
        // This test verifies size is NOT decremented if sweep channel is broken
        let mut cache = CacheController::new(100, 0.5, 1, 10, 100);
        cache.access("file1");
        cache.download_done("file1", 60).unwrap(); // over soft limit (100 * 0.5 = 50)

        // Create a channel then drop receiver immediately — send will fail
        let (tx, _rx) = std::sync::mpsc::channel::<String>();
        drop(_rx);

        let result = cache.sweep_once(&tx);
        // send failed, so sweep_once returns false
        assert!(!result);
        // size must NOT have changed — no decrement on failed send
        let (_, size) = cache.stats();
        assert_eq!(size, 60);
    }

    #[test]
    fn test_no_collision_on_same_timestamp() {
        let mut cache = CacheController::new(10000, 0.9, 60, 10, 1000);
        for i in 0..100 {
            let url = format!("/file{}", i);
            cache.access(&url);
            cache.download_done(&url, 10).unwrap();
        }
        let (entries, size) = cache.stats();
        assert_eq!(entries, 100, "all 100 entries must survive");
        assert_eq!(size, 1000);
    }

    #[test]
    fn test_file_size_limit() {
        let mut cache_controller = CacheController::new(1000, 0.8, 1, 10, 50);
        
        // Test file size limit
        cache_controller.access("large_file");
        let result = cache_controller.download_done("large_file", 100); // Exceeds limit
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::Download { message } => {
                assert!(message.contains("File too large"));
            }
            _ => panic!("Expected Download error"),
        }
    }
}
