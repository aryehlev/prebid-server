//! Stored requests fetching and caching.
//! Mirrors Go `stored_requests/fetcher.go`.
//!
//! Provides traits and types for fetching stored request/response/account
//! data from backends (databases, files, HTTP) with an optional caching layer.

use std::collections::HashMap;

/// Raw JSON data type for stored items.
pub type RawJson = serde_json::Value;

/// Fetcher knows how to fetch Stored Request data by id.
/// Implementations must be safe for concurrent access.
/// Mirrors Go `stored_requests.Fetcher`.
pub trait Fetcher: Send + Sync {
    /// Fetch stored requests and stored imps by their IDs.
    fn fetch_requests(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> Result<(HashMap<String, RawJson>, HashMap<String, RawJson>), Vec<String>>;

    /// Fetch stored responses by their IDs.
    fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, RawJson>, Vec<String>>;
}

/// AccountFetcher fetches host account configuration for a publisher.
/// Mirrors Go `stored_requests.AccountFetcher`.
pub trait AccountFetcher: Send + Sync {
    /// Fetch account configuration JSON. Returns the raw JSON for the account.
    fn fetch_account(
        &self,
        account_default_json: &[u8],
        account_id: &str,
    ) -> Result<Vec<u8>, Vec<String>>;
}

/// CategoryFetcher fetches ad-server/publisher specific categories.
/// Mirrors Go `stored_requests.CategoryFetcher`.
pub trait CategoryFetcher: Send + Sync {
    fn fetch_categories(
        &self,
        primary_ad_server: &str,
        publisher_id: &str,
        iab_category: &str,
    ) -> Result<String, String>;
}

/// NotFoundError indicates a stored item was not found.
/// Mirrors Go `stored_requests.NotFoundError`.
#[derive(Debug, Clone)]
pub struct NotFoundError {
    pub id: String,
    pub data_type: String,
}

impl std::fmt::Display for NotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Stored {} with ID=\"{}\" not found.",
            self.data_type, self.id
        )
    }
}

impl std::error::Error for NotFoundError {}

/// Category represents an ad category mapping.
#[derive(Debug, Clone)]
pub struct Category {
    pub id: String,
    pub name: String,
}

/// CacheJSON is an interface for caching stored request/response/account data.
/// Mirrors Go `stored_requests.CacheJSON`.
pub trait CacheJson: Send + Sync {
    /// Get cached data for the given IDs. Missing entries are simply omitted.
    fn get(&self, ids: &[String]) -> HashMap<String, RawJson>;

    /// Invalidate cached entries for the given IDs.
    fn invalidate(&self, ids: &[String]);

    /// Save data to the cache, adding or overwriting entries.
    fn save(&self, data: &HashMap<String, RawJson>);
}

/// Cache holds cache layers for different data types.
/// Mirrors Go `stored_requests.Cache`.
pub struct Cache {
    pub requests: Box<dyn CacheJson>,
    pub imps: Box<dyn CacheJson>,
    pub responses: Box<dyn CacheJson>,
    pub accounts: Box<dyn CacheJson>,
}

/// A no-op cache that never stores or returns anything.
pub struct NoopCache;

impl CacheJson for NoopCache {
    fn get(&self, _ids: &[String]) -> HashMap<String, RawJson> {
        HashMap::new()
    }

    fn invalidate(&self, _ids: &[String]) {}

    fn save(&self, _data: &HashMap<String, RawJson>) {}
}

/// Find IDs that are not present in the cached data.
fn find_leftovers(ids: &[String], data: &HashMap<String, RawJson>) -> Vec<String> {
    ids.iter()
        .filter(|id| !data.contains_key(*id))
        .cloned()
        .collect()
}

/// Merge fetched data into cached data.
fn merge_data(
    cached: &mut HashMap<String, RawJson>,
    fetched: HashMap<String, RawJson>,
) {
    for (key, value) in fetched {
        cached.insert(key, value);
    }
}

/// A fetcher with an intermediate cache layer.
/// Mirrors Go `fetcherWithCache`.
pub struct FetcherWithCache<F: Fetcher + AccountFetcher + CategoryFetcher> {
    pub fetcher: F,
    pub cache: Cache,
}

impl<F: Fetcher + AccountFetcher + CategoryFetcher> Fetcher for FetcherWithCache<F> {
    fn fetch_requests(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> Result<(HashMap<String, RawJson>, HashMap<String, RawJson>), Vec<String>> {
        let mut request_data = self.cache.requests.get(request_ids);
        let mut imp_data = self.cache.imps.get(imp_ids);

        let leftover_reqs = find_leftovers(request_ids, &request_data);
        let leftover_imps = find_leftovers(imp_ids, &imp_data);

        if !leftover_reqs.is_empty() || !leftover_imps.is_empty() {
            let (fetched_reqs, fetched_imps) =
                self.fetcher.fetch_requests(&leftover_reqs, &leftover_imps)?;

            self.cache.requests.save(&fetched_reqs);
            self.cache.imps.save(&fetched_imps);

            merge_data(&mut request_data, fetched_reqs);
            merge_data(&mut imp_data, fetched_imps);
        }

        Ok((request_data, imp_data))
    }

    fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, RawJson>, Vec<String>> {
        let mut data = self.cache.responses.get(ids);
        let leftovers = find_leftovers(ids, &data);

        if !leftovers.is_empty() {
            let fetched = self.fetcher.fetch_responses(&leftovers)?;
            self.cache.responses.save(&fetched);
            merge_data(&mut data, fetched);
        }

        Ok(data)
    }
}

impl<F: Fetcher + AccountFetcher + CategoryFetcher> AccountFetcher for FetcherWithCache<F> {
    fn fetch_account(
        &self,
        account_default_json: &[u8],
        account_id: &str,
    ) -> Result<Vec<u8>, Vec<String>> {
        let cached = self.cache.accounts.get(&[account_id.to_string()]);
        if let Some(account_val) = cached.get(account_id) {
            return serde_json::to_vec(account_val)
                .map_err(|e| vec![e.to_string()]);
        }

        let result = self.fetcher.fetch_account(account_default_json, account_id)?;

        // Cache the fetched account
        if let Ok(val) = serde_json::from_slice::<RawJson>(&result) {
            let mut data = HashMap::new();
            data.insert(account_id.to_string(), val);
            self.cache.accounts.save(&data);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let err = NotFoundError {
            id: "abc123".to_string(),
            data_type: "request".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Stored request with ID=\"abc123\" not found."
        );
    }

    #[test]
    fn test_find_leftovers() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut data = HashMap::new();
        data.insert("a".to_string(), serde_json::json!({}));

        let leftovers = find_leftovers(&ids, &data);
        assert_eq!(leftovers, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_find_leftovers_all_cached() {
        let ids = vec!["a".to_string()];
        let mut data = HashMap::new();
        data.insert("a".to_string(), serde_json::json!({}));

        let leftovers = find_leftovers(&ids, &data);
        assert!(leftovers.is_empty());
    }

    #[test]
    fn test_merge_data() {
        let mut cached = HashMap::new();
        cached.insert("a".to_string(), serde_json::json!(1));

        let mut fetched = HashMap::new();
        fetched.insert("b".to_string(), serde_json::json!(2));
        fetched.insert("a".to_string(), serde_json::json!(3)); // overwrites

        merge_data(&mut cached, fetched);
        assert_eq!(cached.len(), 2);
        assert_eq!(cached["a"], serde_json::json!(3));
        assert_eq!(cached["b"], serde_json::json!(2));
    }

    #[test]
    fn test_noop_cache() {
        let cache = NoopCache;
        assert!(cache.get(&["id".to_string()]).is_empty());
        cache.invalidate(&["id".to_string()]); // should not panic
        cache.save(&HashMap::new()); // should not panic
    }
}
