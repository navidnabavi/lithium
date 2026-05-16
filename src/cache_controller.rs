use crate::backend::StorageBackend;
use crate::config::SweeperConfig;
use crate::error::{LithiumError, Result};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

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
    max_file_size: usize,
}

impl CacheController {
    pub fn new(max_file_size: usize) -> Self {
        Self {
            urls: BTreeMap::new(),
            url_map: HashMap::new(),
            downloading: Arc::new(Mutex::new(HashSet::new())),
            size: 0,
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
                message: format!(
                    "File too large: {} bytes (max: {})",
                    size, self.max_file_size
                ),
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
                message: format!("URL {} not found in cache", url),
            })
        }
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.url_map.len(), self.size)
    }

    pub fn dump(&self) {
        info!(
            "Cache stats: {} entries, {} bytes",
            self.url_map.len(),
            self.size
        );
        for (url, time_url) in &self.url_map {
            debug!(
                "  {}: {} bytes, accessed at {}",
                url, time_url.size, time_url.time
            );
        }
    }

    fn sweep(
        &mut self,
        file_deleter: &UnboundedSender<String>,
        size_limit: usize,
        soft_limit_ratio: f64,
        max_delete_per_iteration: usize,
        sweep_interval_secs: u64,
    ) -> u64 {
        let mut deleted = 0;

        while deleted < max_delete_per_iteration
            && self.soft_limit_passed(size_limit, soft_limit_ratio)
        {
            if self.sweep_once(file_deleter) {
                deleted += 1;
            } else {
                break; // No more items to delete
            }
        }

        if deleted > 0 {
            info!("Swept {} items from cache", deleted);
        }

        sweep_interval_secs
    }

    fn get_oldest(&self) -> Option<(u64, String)> {
        self.urls
            .iter()
            .next()
            .map(|((time, url), _)| (*time, url.clone()))
    }

    fn sweep_once(&mut self, file_deleter: &UnboundedSender<String>) -> bool {
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

    pub fn soft_limit_passed(&self, size_limit: usize, soft_limit_ratio: f64) -> bool {
        self.size > (size_limit as f64 * soft_limit_ratio) as usize
    }

    pub fn hard_limit_passed(&self, size_limit: usize) -> bool {
        self.size > size_limit
    }
}

pub struct Sweeper {
    sweeper_handle: JoinHandle<()>,
    file_deleter_handle: JoinHandle<()>,
}

impl Sweeper {
    pub fn new(
        cache_controller: Arc<RwLock<CacheController>>,
        backend: Arc<dyn StorageBackend>,
        stop: Arc<AtomicBool>,
        cfg: SweeperConfig,
    ) -> Self {
        let (tx, mut rx) = unbounded_channel::<String>();

        let stop_sweeper = stop.clone();
        let sweeper_handle = tokio::spawn(async move {
            info!("Sweeper task started");
            while !stop_sweeper.load(Ordering::Relaxed) {
                let delay_secs = match cache_controller.write() {
                    Ok(mut cache) => cache.sweep(
                        &tx,
                        cfg.size_limit,
                        cfg.soft_limit_ratio,
                        cfg.max_delete_per_iteration,
                        cfg.sweep_interval_secs,
                    ),
                    Err(e) => {
                        error!("Failed to acquire cache lock in sweeper: {}", e);
                        60
                    }
                };
                // Sleep in 100ms increments to check stop flag frequently
                let mut elapsed = 0u64;
                while elapsed < delay_secs * 1000 && !stop_sweeper.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    elapsed += 100;
                }
            }
            info!("Sweeper task stopped");
        });

        let file_deleter_handle = tokio::spawn(async move {
            info!("File deleter task started");
            while let Some(path) = rx.recv().await {
                if let Err(e) = backend.delete(&path).await {
                    error!("Failed to delete {}: {}", path, e);
                }
            }
            info!("File deleter task stopped");
        });

        Self {
            sweeper_handle,
            file_deleter_handle,
        }
    }

    pub async fn join(self) {
        let _ = self.sweeper_handle.await;
        let _ = self.file_deleter_handle.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache() {
        let mut cache_controller = CacheController::new(100);

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
        let mut cache = CacheController::new(100);
        cache.access("file1");
        cache.download_done("file1", 60).unwrap(); // over soft limit (100 * 0.5 = 50)

        // Create a channel then drop receiver immediately — send will fail
        let (tx, _rx) = unbounded_channel::<String>();
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
        let mut cache = CacheController::new(1000);
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
        let mut cache_controller = CacheController::new(50);

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

    #[test]
    fn test_cache_controller_new_takes_only_max_file_size() {
        let cache = CacheController::new(100);
        let (entries, size) = cache.stats();
        assert_eq!(entries, 0);
        assert_eq!(size, 0);
    }

    #[test]
    fn test_soft_limit_passed_takes_params() {
        let mut cache = CacheController::new(100);
        cache.access("file1");
        cache.download_done("file1", 60).unwrap();
        // size_limit=100, ratio=0.5 → threshold=50; size=60 > 50 → true
        assert!(cache.soft_limit_passed(100, 0.5));
        // size_limit=100, ratio=0.9 → threshold=90; size=60 < 90 → false
        assert!(!cache.soft_limit_passed(100, 0.9));
    }

    #[test]
    fn test_sweep_takes_params() {
        let mut cache = CacheController::new(100);
        cache.access("file1");
        cache.download_done("file1", 60).unwrap();

        let (tx, mut rx) = unbounded_channel::<String>();
        // size_limit=100, ratio=0.5 → over soft limit; max_delete=10; interval=5
        let interval = cache.sweep(&tx, 100, 0.5, 10, 5);
        assert_eq!(interval, 5);

        let deleted = rx.try_recv().unwrap();
        assert_eq!(deleted, "file1");
        let (entries, size) = cache.stats();
        assert_eq!(entries, 0);
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_sweeper_evicts_when_enabled() {
        use crate::config::SweeperConfig;
        use bytes::Bytes;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct NoopBackend;

        #[async_trait::async_trait]
        impl crate::backend::StorageBackend for NoopBackend {
            async fn store(&self, _path: &str, _data: Bytes) -> crate::error::Result<usize> {
                Ok(0)
            }
            async fn delete(&self, _path: &str) -> crate::error::Result<()> {
                Ok(())
            }
            fn accel_redirect_path(&self, path: &str) -> String {
                path.to_string()
            }
        }

        let cfg = SweeperConfig {
            enabled: true,
            size_limit: 100,
            soft_limit_ratio: 0.5, // threshold: 50 bytes
            sweep_interval_secs: 1,
            max_delete_per_iteration: 10,
        };

        let cache = Arc::new(RwLock::new(CacheController::new(200)));
        {
            let mut c = cache.write().unwrap();
            c.access("file1");
            c.download_done("file1", 80).unwrap(); // 80 > 50 → over soft limit
        }

        let stop = Arc::new(AtomicBool::new(false));
        let backend: Arc<dyn crate::backend::StorageBackend> = Arc::new(NoopBackend);
        let sweeper = Sweeper::new(cache.clone(), backend, stop.clone(), cfg);

        // Let sweeper run (interval=1s, check after 300ms — sweeper runs immediately on first iteration)
        tokio::time::sleep(Duration::from_millis(300)).await;

        let (entries, _) = cache.read().unwrap().stats();
        assert_eq!(entries, 0, "sweeper should have evicted file1");

        stop.store(true, Ordering::Relaxed);
        sweeper.join().await;
    }

    #[test]
    fn test_sweeper_disabled_cache_retains_entries() {
        // When no Sweeper is created (disabled), cache retains entries regardless of size
        let mut cache = CacheController::new(100);
        cache.access("file1");
        cache.download_done("file1", 80).unwrap();

        let (entries, size) = cache.stats();
        assert_eq!(entries, 1);
        assert_eq!(size, 80);
    }
}
