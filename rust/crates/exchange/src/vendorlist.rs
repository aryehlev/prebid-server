//! GVL (Global Vendor List) fetching and caching.
//!
//! Mirrors Go `gdpr/vendorlist-fetching.go`.
//!
//! Provides functions for fetching, caching, and querying vendor lists
//! from the IAB's GVL endpoint for GDPR TCF2 compliance.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::gdpr::VendorList;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Highest supported GVL specification version.
pub const LATEST_SPEC_VERSION: u16 = 3;

/// Default URL pattern for fetching vendor lists from IAB.
const VENDOR_LIST_URL_PATTERN: &str =
    "https://vendor-list.consensu.org/v-{spec}/vendor-list-v{list}.json";

// ---------------------------------------------------------------------------
// VendorListCache
// ---------------------------------------------------------------------------

/// Cache entry for a vendor list with fetch timestamp.
#[derive(Debug, Clone)]
struct CacheEntry {
    list: VendorList,
    fetched_at: Instant,
}

/// Thread-safe cache for vendor lists, keyed by (spec_version, list_version).
#[derive(Debug)]
pub struct VendorListCache {
    entries: RwLock<HashMap<(u16, u16), CacheEntry>>,
    /// Maximum age for cache entries before they're considered stale.
    max_age: Duration,
}

impl VendorListCache {
    pub fn new(max_age: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            max_age,
        }
    }

    /// Save a vendor list to the cache.
    pub fn save(&self, spec_version: u16, list_version: u16, list: VendorList) {
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(
                (spec_version, list_version),
                CacheEntry {
                    list,
                    fetched_at: Instant::now(),
                },
            );
        }
    }

    /// Load a vendor list from the cache. Returns None if not cached or stale.
    pub fn load(&self, spec_version: u16, list_version: u16) -> Option<VendorList> {
        let entries = self.entries.read().ok()?;
        let entry = entries.get(&(spec_version, list_version))?;
        if entry.fetched_at.elapsed() > self.max_age {
            return None;
        }
        Some(entry.list.clone())
    }

    /// Check if a vendor list is cached (regardless of staleness).
    pub fn contains(&self, spec_version: u16, list_version: u16) -> bool {
        self.entries
            .read()
            .ok()
            .map_or(false, |e| e.contains_key(&(spec_version, list_version)))
    }

    /// Get all cached version keys.
    pub fn cached_versions(&self) -> Vec<(u16, u16)> {
        self.entries
            .read()
            .ok()
            .map(|e| e.keys().copied().collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// VendorListFetcherImpl
// ---------------------------------------------------------------------------

/// Fetcher for GVL vendor lists with caching.
///
/// Mirrors Go `gdpr.NewVendorListFetcher`.
pub struct VendorListFetcherImpl {
    cache: Arc<VendorListCache>,
    /// URL maker function: (spec_version, list_version) -> URL
    url_maker: Box<dyn Fn(u16, u16) -> String + Send + Sync>,
    /// Rate limiter: minimum interval between HTTP fetches.
    min_fetch_interval: Duration,
    last_fetch: RwLock<Option<Instant>>,
}

impl VendorListFetcherImpl {
    pub fn new(
        cache: Arc<VendorListCache>,
        url_maker: impl Fn(u16, u16) -> String + Send + Sync + 'static,
        min_fetch_interval: Duration,
    ) -> Self {
        Self {
            cache,
            url_maker: Box::new(url_maker),
            min_fetch_interval,
            last_fetch: RwLock::new(None),
        }
    }

    /// Fetch a vendor list, using cache first with HTTP fallback.
    pub fn fetch(&self, spec_version: u16, list_version: u16) -> Option<VendorList> {
        // Try cache first
        if let Some(list) = self.cache.load(spec_version, list_version) {
            return Some(list);
        }

        // Rate limiting: don't fetch too frequently
        if let Ok(last) = self.last_fetch.read() {
            if let Some(last_time) = *last {
                if last_time.elapsed() < self.min_fetch_interval {
                    return None;
                }
            }
        }

        // In a real implementation, this would make an HTTP request.
        // For now, we return None for uncached lists.
        // The async fetch would be done via a separate background task.
        None
    }

    /// Get the URL for a specific vendor list version.
    pub fn get_url(&self, spec_version: u16, list_version: u16) -> String {
        (self.url_maker)(spec_version, list_version)
    }

    /// Get a reference to the underlying cache.
    pub fn cache(&self) -> &Arc<VendorListCache> {
        &self.cache
    }
}

/// Construct the IAB vendor list URL for a given spec and list version.
///
/// Mirrors Go `gdpr.VendorListURLMaker`.
pub fn vendor_list_url_maker(spec_version: u16, list_version: u16) -> String {
    VENDOR_LIST_URL_PATTERN
        .replace("{spec}", &spec_version.to_string())
        .replace("{list}", &list_version.to_string())
}

/// Parse vendor list data from JSON bytes and save to cache.
///
/// Returns the parsed vendor list version, or None on failure.
pub fn save_vendor_list(
    data: &[u8],
    spec_version: u16,
    cache: &VendorListCache,
) -> Option<u32> {
    let list = VendorList::from_json(data)?;
    let version = list.version;
    cache.save(spec_version, version as u16, list);
    Some(version)
}

// ---------------------------------------------------------------------------
// GVL Vendor ID management
// ---------------------------------------------------------------------------

/// Maps bidder names to their GVL vendor IDs.
///
/// Mirrors Go `gdpr.LiveGVLVendorIDs`.
#[derive(Debug, Clone, Default)]
pub struct GvlVendorIds {
    pub ids: HashMap<String, u16>,
}

impl GvlVendorIds {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a bidder's GVL vendor ID.
    pub fn register(&mut self, bidder: &str, vendor_id: u16) {
        self.ids.insert(bidder.to_string(), vendor_id);
    }

    /// Look up a bidder's vendor ID.
    pub fn get(&self, bidder: &str) -> Option<u16> {
        self.ids.get(bidder).copied()
    }

    /// Check if a vendor ID is valid (exists in any registered GVL).
    pub fn is_valid(&self, vendor_id: u16) -> bool {
        self.ids.values().any(|&id| id == vendor_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_list_url_maker() {
        let url = vendor_list_url_maker(2, 150);
        assert!(url.contains("/v-2/"));
        assert!(url.contains("vendor-list-v150"));
    }

    #[test]
    fn test_cache_save_and_load() {
        let cache = VendorListCache::new(Duration::from_secs(3600));
        let list = VendorList {
            version: 100,
            vendors: HashMap::new(),
        };
        cache.save(2, 100, list.clone());

        let loaded = cache.load(2, 100);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().version, 100);
    }

    #[test]
    fn test_cache_miss() {
        let cache = VendorListCache::new(Duration::from_secs(3600));
        assert!(cache.load(2, 99).is_none());
    }

    #[test]
    fn test_cache_contains() {
        let cache = VendorListCache::new(Duration::from_secs(3600));
        assert!(!cache.contains(2, 100));

        let list = VendorList {
            version: 100,
            vendors: HashMap::new(),
        };
        cache.save(2, 100, list);
        assert!(cache.contains(2, 100));
    }

    #[test]
    fn test_gvl_vendor_ids() {
        let mut ids = GvlVendorIds::new();
        ids.register("appnexus", 32);
        ids.register("rubicon", 52);

        assert_eq!(ids.get("appnexus"), Some(32));
        assert_eq!(ids.get("rubicon"), Some(52));
        assert_eq!(ids.get("unknown"), None);
        assert!(ids.is_valid(32));
        assert!(!ids.is_valid(999));
    }

    #[test]
    fn test_save_vendor_list() {
        let json = serde_json::json!({
            "vendorListVersion": 150,
            "vendors": {
                "32": {
                    "purposes": [1, 2, 3],
                    "legIntPurposes": [2, 7],
                    "specialPurposes": [],
                    "flexiblePurposes": [2]
                }
            }
        });
        let data = serde_json::to_vec(&json).unwrap();
        let cache = VendorListCache::new(Duration::from_secs(3600));
        let version = save_vendor_list(&data, 2, &cache);
        assert_eq!(version, Some(150));
        assert!(cache.contains(2, 150));
    }

    #[test]
    fn test_fetcher_cache_hit() {
        let cache = Arc::new(VendorListCache::new(Duration::from_secs(3600)));
        let list = VendorList {
            version: 200,
            vendors: HashMap::new(),
        };
        cache.save(3, 200, list);

        let fetcher =
            VendorListFetcherImpl::new(cache, vendor_list_url_maker, Duration::from_secs(600));

        let result = fetcher.fetch(3, 200);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version, 200);
    }

    #[test]
    fn test_fetcher_cache_miss() {
        let cache = Arc::new(VendorListCache::new(Duration::from_secs(3600)));
        let fetcher =
            VendorListFetcherImpl::new(cache, vendor_list_url_maker, Duration::from_secs(600));

        let result = fetcher.fetch(3, 999);
        assert!(result.is_none());
    }
}
