use std::collections::{HashMap, HashSet};
use std::fmt;

use rand::seq::SliceRandom;
use regex::Regex;
use serde::{Deserialize, Serialize};

use pbs_config::{
    BidderInfo, BidderSyncerConfig, Configuration, SyncerEndpointConfig, UserSyncConfig,
};

/// Represents a syncer configuration for a bidder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncerInfo {
    pub bidder: String,
    pub sync_type: SyncType,
    pub iframe_url: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    Iframe,
    Redirect,
    Both,
}

/// User UID stored in prebid cookie
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UidEntry {
    pub uid: String,
    pub expires: Option<String>,
}

/// The prebid cookie stores UIDs per bidder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrebidCookie {
    pub uids: HashMap<String, UidEntry>,
    pub opt_out: bool,
}

impl PrebidCookie {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_uid(&self, bidder: &str) -> Option<&UidEntry> {
        self.uids.get(bidder)
    }

    pub fn set_uid(&mut self, bidder: String, uid: String) {
        self.uids.insert(bidder, UidEntry { uid, expires: None });
    }

    pub fn opt_out(&self) -> bool {
        self.opt_out
    }

    /// Parse from cookie string (base64-encoded JSON)
    pub fn from_cookie(cookie_val: &str) -> Self {
        // Try base64 decode then JSON parse
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(cookie_val)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(cookie_val));
        if let Ok(bytes) = decoded {
            if let Ok(cookie) = serde_json::from_slice(&bytes) {
                return cookie;
            }
        }
        // Try direct JSON
        serde_json::from_str(cookie_val).unwrap_or_default()
    }

    /// Encode to base64 JSON for cookie
    pub fn to_cookie_string(&self) -> String {
        use base64::Engine;
        let json = serde_json::to_string(self).unwrap_or_default();
        base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
    }
}

/// Syncer determines the sync URL for a bidder
#[derive(Debug)]
pub struct Syncer {
    pub bidder: String,
    pub iframe_url: Option<String>,
    pub redirect_url: Option<String>,
}

impl Syncer {
    /// The syncer key used to store/lookup UIDs in the cookie.
    /// This is often the bidder name, but can be shared across bidders.
    pub fn key(&self) -> &str {
        &self.bidder
    }

    /// The default response format based on which endpoints are configured
    /// and any format override.
    pub fn default_response_format(&self) -> SyncType {
        if self.iframe_url.is_some() {
            SyncType::Iframe
        } else {
            SyncType::Redirect
        }
    }

    /// Returns true if the syncer supports at least one of the given sync types.
    pub fn supports_type(&self, sync_types: &[SyncType]) -> bool {
        for st in sync_types {
            match st {
                SyncType::Iframe if self.iframe_url.is_some() => return true,
                SyncType::Redirect if self.redirect_url.is_some() => return true,
                _ => {}
            }
        }
        false
    }

    pub fn get_sync_url(&self, sync_type: &SyncType, gdpr: i32, consent: &str) -> Option<String> {
        let base = match sync_type {
            SyncType::Iframe => self.iframe_url.as_deref()?,
            SyncType::Redirect | SyncType::Both => self.redirect_url.as_deref()?,
        };
        let url = base
            .replace("{gdpr}", &gdpr.to_string())
            .replace("{gdpr_consent}", consent)
            .replace("{{gdpr}}", &gdpr.to_string())
            .replace("{{gdpr_consent}}", consent);
        Some(url)
    }
}

// ---------------------------------------------------------------------------
// BidderChooser – selects which bidders need syncing
// ---------------------------------------------------------------------------

/// Configuration for choosing bidders to sync
#[derive(Debug, Clone)]
pub struct ChooserConfig {
    pub max_syncs_per_request: usize,
    pub cooperative_sync_enabled: bool,
    pub cooperative_sync_priority: Vec<String>,
}

impl Default for ChooserConfig {
    fn default() -> Self {
        Self {
            max_syncs_per_request: 8,
            cooperative_sync_enabled: true,
            cooperative_sync_priority: Vec::new(),
        }
    }
}

/// Result of choosing bidders for syncing
#[derive(Debug, Clone, Default)]
pub struct ChooseResult {
    pub syncs: Vec<SyncerChoice>,
    pub rejected: Vec<RejectedSyncer>,
}

#[derive(Debug, Clone)]
pub struct SyncerChoice {
    pub bidder: String,
    pub sync_type: SyncType,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct RejectedSyncer {
    pub bidder: String,
    pub reason: RejectionReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectionReason {
    AlreadySynced,
    OptedOut,
    GdprBlocked,
    TypeNotSupported,
    MaxSyncsReached,
    BidderDisabled,
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::AlreadySynced => write!(f, "already synced"),
            Self::OptedOut => write!(f, "user opted out"),
            Self::GdprBlocked => write!(f, "blocked by GDPR"),
            Self::TypeNotSupported => write!(f, "sync type not supported"),
            Self::MaxSyncsReached => write!(f, "max syncs reached"),
            Self::BidderDisabled => write!(f, "bidder disabled"),
        }
    }
}

/// Choose which bidders should be included in a cookie sync response
pub fn choose_bidders(
    cookie: &PrebidCookie,
    available_syncers: &HashMap<String, Syncer>,
    requested_bidders: &[String],
    gdpr_applies: bool,
    consent_string: &str,
    config: &ChooserConfig,
) -> ChooseResult {
    let mut result = ChooseResult::default();

    if cookie.opt_out() {
        for bidder in requested_bidders {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::OptedOut,
            });
        }
        return result;
    }

    let bidders_to_check: Vec<&String> = if requested_bidders.is_empty() {
        // Cooperative sync: use all available syncers
        if config.cooperative_sync_enabled {
            available_syncers.keys().collect()
        } else {
            return result;
        }
    } else {
        requested_bidders.iter().collect()
    };

    for bidder in bidders_to_check {
        if result.syncs.len() >= config.max_syncs_per_request {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::MaxSyncsReached,
            });
            continue;
        }

        // Check if already synced
        if cookie.get_uid(bidder).is_some() {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::AlreadySynced,
            });
            continue;
        }

        // Check GDPR
        if gdpr_applies && consent_string.is_empty() {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::GdprBlocked,
            });
            continue;
        }

        // Get syncer
        let syncer = match available_syncers.get(bidder.as_str()) {
            Some(s) => s,
            None => {
                result.rejected.push(RejectedSyncer {
                    bidder: bidder.clone(),
                    reason: RejectionReason::BidderDisabled,
                });
                continue;
            }
        };

        // Determine sync type and URL
        let (sync_type, url) = if let Some(url) = syncer.get_sync_url(
            &SyncType::Iframe,
            if gdpr_applies { 1 } else { 0 },
            consent_string,
        ) {
            (SyncType::Iframe, url)
        } else if let Some(url) = syncer.get_sync_url(
            &SyncType::Redirect,
            if gdpr_applies { 1 } else { 0 },
            consent_string,
        ) {
            (SyncType::Redirect, url)
        } else {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::TypeNotSupported,
            });
            continue;
        };

        result.syncs.push(SyncerChoice {
            bidder: bidder.clone(),
            sync_type,
            url,
        });
    }

    result
}

// ---------------------------------------------------------------------------
// Cookie sync response types
// ---------------------------------------------------------------------------

/// Cookie sync response returned to the client
#[derive(Debug, Clone, Serialize)]
pub struct CookieSyncResponse {
    pub status: CookieSyncStatus,
    #[serde(rename = "bidder_status")]
    pub bidder_status: Vec<BidderSyncStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CookieSyncStatus {
    Ok,
    NoCookie,
    OptOut,
}

#[derive(Debug, Clone, Serialize)]
pub struct BidderSyncStatus {
    pub bidder: String,
    #[serde(rename = "usersync")]
    pub usersync: Option<UsersyncInfo>,
    pub error: Option<String>,
    #[serde(rename = "no_cookie")]
    pub no_cookie: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsersyncInfo {
    pub url: String,
    #[serde(rename = "type")]
    pub sync_type: String,
    #[serde(rename = "supportCORS")]
    pub support_cors: bool,
}

/// Build a CookieSyncResponse from a ChooseResult
pub fn build_sync_response(
    cookie: &PrebidCookie,
    choose_result: &ChooseResult,
) -> CookieSyncResponse {
    let status = if cookie.opt_out() {
        CookieSyncStatus::OptOut
    } else if cookie.uids.is_empty() {
        CookieSyncStatus::NoCookie
    } else {
        CookieSyncStatus::Ok
    };

    let bidder_status = choose_result
        .syncs
        .iter()
        .map(|s| BidderSyncStatus {
            bidder: s.bidder.clone(),
            usersync: Some(UsersyncInfo {
                url: s.url.clone(),
                sync_type: match s.sync_type {
                    SyncType::Iframe => "iframe".to_string(),
                    SyncType::Redirect => "redirect".to_string(),
                    SyncType::Both => "redirect".to_string(),
                },
                support_cors: true,
            }),
            error: None,
            no_cookie: true,
        })
        .collect();

    CookieSyncResponse {
        status,
        bidder_status,
    }
}

// ---------------------------------------------------------------------------
// BidderFilter – determines if a bidder has permission to sync
// ---------------------------------------------------------------------------

/// Filter mode: include means the listed bidders are allowed; exclude means
/// the listed bidders are disallowed and all others are allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidderFilterMode {
    Include,
    Exclude,
}

/// Determines if a bidder has permission to perform a user sync activity.
pub trait BidderFilter {
    fn allowed(&self, bidder: &str) -> bool;
}

/// Applies the mode for a specific set of bidders.
#[derive(Debug, Clone)]
pub struct SpecificBidderFilter {
    bidders_lookup: HashSet<String>,
    mode: BidderFilterMode,
}

impl SpecificBidderFilter {
    pub fn new(bidders: &[&str], mode: BidderFilterMode) -> Self {
        Self {
            bidders_lookup: bidders.iter().map(|b| b.to_lowercase()).collect(),
            mode,
        }
    }
}

impl BidderFilter for SpecificBidderFilter {
    fn allowed(&self, bidder: &str) -> bool {
        let exists = self.bidders_lookup.contains(&bidder.to_lowercase());
        match self.mode {
            BidderFilterMode::Include => exists,
            BidderFilterMode::Exclude => !exists,
        }
    }
}

/// Applies the same mode uniformly for all bidders.
#[derive(Debug, Clone)]
pub struct UniformBidderFilter {
    mode: BidderFilterMode,
}

impl UniformBidderFilter {
    pub fn new(mode: BidderFilterMode) -> Self {
        Self { mode }
    }
}

impl BidderFilter for UniformBidderFilter {
    fn allowed(&self, _bidder: &str) -> bool {
        self.mode == BidderFilterMode::Include
    }
}

// ---------------------------------------------------------------------------
// SyncTypeFilter – determines which sync types are allowed per bidder
// ---------------------------------------------------------------------------

/// Determines which sync types a bidder is permitted to use.
pub struct SyncTypeFilter {
    pub iframe: Box<dyn BidderFilter>,
    pub redirect: Box<dyn BidderFilter>,
}

impl SyncTypeFilter {
    pub fn new(iframe: Box<dyn BidderFilter>, redirect: Box<dyn BidderFilter>) -> Self {
        Self { iframe, redirect }
    }

    /// Returns the list of sync types permitted for the given bidder.
    pub fn for_bidder(&self, bidder: &str) -> Vec<SyncType> {
        let mut sync_types = Vec::new();
        if self.iframe.allowed(bidder) {
            sync_types.push(SyncType::Iframe);
        }
        if self.redirect.allowed(bidder) {
            sync_types.push(SyncType::Redirect);
        }
        sync_types
    }
}

// ---------------------------------------------------------------------------
// Ejector – chooses which UID to remove when the cookie is full
// ---------------------------------------------------------------------------

/// Chooses which UID entry to eject from the cookie.
pub trait Ejector {
    fn choose(&mut self, uids: &HashMap<String, UidEntry>) -> Result<String, String>;
}

/// Ejects the UID with the oldest (earliest) expiry time.
pub struct OldestEjector;

impl Ejector for OldestEjector {
    fn choose(&mut self, uids: &HashMap<String, UidEntry>) -> Result<String, String> {
        if uids.is_empty() {
            return Err("no uids to eject".to_string());
        }

        let mut oldest_key: Option<&str> = None;
        // Use Option<Option<&str>> so we can distinguish "no entry chosen" (None)
        // from "entry chosen with no expiry" (Some(None)).
        let mut oldest_expires: Option<Option<&str>> = None;

        for (key, entry) in uids {
            let entry_exp = entry.expires.as_deref();
            let is_older = match (&oldest_expires, &entry_exp) {
                // No entry chosen yet
                (None, _) => true,
                // Current has expiry, new has none – no-expiry is treated as oldest
                (Some(Some(_)), None) => true,
                // Current has no expiry, new has one – keep current (no expiry is oldest)
                (Some(None), Some(_)) => false,
                // Both have no expiry – keep first
                (Some(None), None) => false,
                // Both have expiry – earlier string wins
                (Some(Some(cur)), Some(new)) => *new < *cur,
            };
            if is_older {
                oldest_key = Some(key.as_str());
                oldest_expires = Some(entry_exp);
            }
        }

        oldest_key
            .map(|k| k.to_string())
            .ok_or_else(|| "no uids to eject".to_string())
    }
}

/// Ejects non-priority UIDs first, then the oldest UID in the lowest-priority
/// group. Mirrors the Go `PriorityBidderEjector`.
pub struct PriorityBidderEjector {
    pub priority_groups: Vec<Vec<String>>,
    pub syncer_keys_by_bidder: HashMap<String, String>,
    pub is_syncer_priority: bool,
    pub tie_ejector: Box<dyn Ejector>,
}

impl PriorityBidderEjector {
    /// Build the set of syncer keys that appear in any priority group.
    fn priority_keys(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for group in &self.priority_groups {
            for bidder in group {
                if let Some(key) = self.syncer_keys_by_bidder.get(bidder) {
                    set.insert(key.clone());
                }
            }
        }
        set
    }

    /// Return only UIDs whose keys are NOT in any priority group.
    fn non_priority_uids<'a>(
        &self,
        uids: &'a HashMap<String, UidEntry>,
    ) -> HashMap<String, UidEntry> {
        if self.priority_groups.is_empty() {
            return uids.clone();
        }
        let pkeys = self.priority_keys();
        uids.iter()
            .filter(|(k, _)| !pkeys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn remove_from_lowest_group(&mut self, element: &str) {
        if let Some(last) = self.priority_groups.last_mut() {
            if last.len() <= 1 {
                self.priority_groups.pop();
            } else {
                last.retain(|e| e != element);
            }
        }
    }
}

impl Ejector for PriorityBidderEjector {
    fn choose(&mut self, uids: &HashMap<String, UidEntry>) -> Result<String, String> {
        let non_priority = self.non_priority_uids(uids);

        // If the current syncer is not priority, and only priority UIDs remain, error
        if non_priority.len() == 1
            && !self.is_syncer_priority
            && !self.priority_groups.is_empty()
        {
            return Err(
                "syncer key is not a priority, and there are only priority elements left"
                    .to_string(),
            );
        }

        if !non_priority.is_empty() {
            return self.tie_ejector.choose(&non_priority);
        }

        // All UIDs are priority – eject from the lowest priority group
        let lowest_group = self
            .priority_groups
            .last()
            .ok_or("no priority groups")?
            .clone();

        if lowest_group.len() == 1 {
            let uid_to_delete = lowest_group[0].clone();
            self.remove_from_lowest_group(&uid_to_delete);
            return Ok(uid_to_delete);
        }

        // Multiple bidders in the lowest group – pick the oldest among them
        let mut lowest_uids: HashMap<String, UidEntry> = HashMap::new();
        for bidder in &lowest_group {
            if let Some(key) = self.syncer_keys_by_bidder.get(bidder) {
                if let Some(entry) = uids.get(key) {
                    lowest_uids.insert(key.clone(), entry.clone());
                }
            }
        }
        let uid_to_delete = self.tie_ejector.choose(&lowest_uids)?;
        self.remove_from_lowest_group(&uid_to_delete);
        Ok(uid_to_delete)
    }
}

// ---------------------------------------------------------------------------
// Cookie encoder / decoder  (base64 URL encoding)
// ---------------------------------------------------------------------------

/// Encode a `PrebidCookie` to a base64-URL-encoded JSON string.
pub fn encode_cookie(cookie: &PrebidCookie) -> Result<String, String> {
    use base64::Engine;
    let json = serde_json::to_string(cookie).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE.encode(json.as_bytes()))
}

/// Decode a base64-URL-encoded JSON string into a `PrebidCookie`.
/// Returns a default cookie on any failure.
pub fn decode_cookie(encoded: &str) -> PrebidCookie {
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::URL_SAFE.decode(encoded) {
        Ok(b) => b,
        Err(_) => return PrebidCookie::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// BidderChooser – cooperative bidder choosing algorithm
// ---------------------------------------------------------------------------

/// Settings governing cooperative syncing behaviour.
#[derive(Debug, Clone)]
pub struct Cooperative {
    pub enabled: bool,
    pub priority_groups: Vec<Vec<String>>,
}

impl Default for Cooperative {
    fn default() -> Self {
        Self {
            enabled: false,
            priority_groups: Vec::new(),
        }
    }
}

/// Chooses an ordered list of bidders to attempt syncing.
///
/// If cooperative sync is enabled the order is:
///   1. requested bidders (shuffled)
///   2. each priority group (shuffled individually)
///   3. all available bidders (shuffled)
///
/// Duplicates across groups are intentional – they are expected to be
/// de-duplicated by the upstream algorithm.
///
/// If cooperative sync is disabled:
///   - return shuffled requested, or shuffled available if none requested.
pub fn choose_bidder_order(
    requested: &[String],
    available: &[String],
    cooperative: &Cooperative,
) -> Vec<String> {
    let mut rng = rand::thread_rng();

    if cooperative.enabled {
        let capacity = (available.len() as f64 * 1.5) as usize;
        let mut bidders: Vec<String> = Vec::with_capacity(capacity);

        // requested (shuffled)
        shuffled_append(&mut bidders, requested, &mut rng);

        // priority groups (each shuffled)
        for group in &cooperative.priority_groups {
            shuffled_append(&mut bidders, group, &mut rng);
        }

        // all available (shuffled)
        shuffled_append(&mut bidders, available, &mut rng);

        bidders
    } else if requested.is_empty() {
        shuffled_copy(available, &mut rng)
    } else {
        shuffled_copy(requested, &mut rng)
    }
}

fn shuffled_copy(src: &[String], rng: &mut impl rand::Rng) -> Vec<String> {
    let mut v: Vec<String> = src.to_vec();
    v.shuffle(rng);
    v
}

fn shuffled_append(dest: &mut Vec<String>, src: &[String], rng: &mut impl rand::Rng) {
    let start = dest.len();
    dest.extend_from_slice(src);
    dest[start..].shuffle(rng);
}

// ---------------------------------------------------------------------------
// Shuffler – randomizes the order of bidder sync attempts
// ---------------------------------------------------------------------------

/// Changes the order of elements in a slice.
///
/// Mirrors Go `shuffler` interface in `usersync/shuffler.go`.
pub trait Shuffler {
    fn shuffle(&self, items: &mut [String]);
}

/// Randomly shuffles elements using `rand::thread_rng`.
///
/// Mirrors Go `randomShuffler` in `usersync/shuffler.go`.
pub struct RandomShuffler;

impl Shuffler for RandomShuffler {
    fn shuffle(&self, items: &mut [String]) {
        let mut rng = rand::thread_rng();
        items.shuffle(&mut rng);
    }
}

// ---------------------------------------------------------------------------
// SyncerBuilder – creates Syncer instances from bidder config
// ---------------------------------------------------------------------------

/// Associates a bidder name with its syncer configuration.
///
/// Mirrors Go `namedSyncerConfig` in `usersync/syncersbuilder.go`.
#[derive(Debug, Clone)]
struct NamedSyncerConfig {
    name: String,
    cfg: BidderSyncerConfig,
}

/// Error returned when a syncer cannot be built for a bidder.
///
/// Mirrors Go `SyncerBuildError` in `usersync/syncersbuilder.go`.
#[derive(Debug, Clone)]
pub struct SyncerBuildError {
    pub bidder: String,
    pub syncer_key: String,
    pub err: String,
}

impl fmt::Display for SyncerBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot create syncer for bidder {} with key {}: {}",
            self.bidder, self.syncer_key, self.err
        )
    }
}

impl std::error::Error for SyncerBuildError {}

/// Build syncers for all enabled bidders.
///
/// This function groups bidder syncer configs by their syncer key, resolves
/// shared keys (multiple bidders sharing one syncer key), and creates a
/// `Syncer` for each bidder.
///
/// Mirrors Go `BuildSyncers()` in `usersync/syncersbuilder.go`.
pub fn build_syncers(
    host_config: &Configuration,
    bidder_infos: &HashMap<String, BidderInfo>,
) -> Result<HashMap<String, Syncer>, Vec<String>> {
    // Map syncer config by bidder
    let mut cfg_by_bidder: HashMap<String, BidderSyncerConfig> = HashMap::new();
    for (bidder, info) in bidder_infos {
        if should_create_syncer(info) {
            if let Some(ref syncer_cfg) = info.user_sync {
                cfg_by_bidder.insert(bidder.clone(), syncer_cfg.clone());
            }
        }
    }

    // Map syncer config by key
    let mut cfg_by_syncer_key: HashMap<String, Vec<NamedSyncerConfig>> = HashMap::new();
    for (bidder, mut cfg) in cfg_by_bidder {
        if cfg.key.is_empty() {
            cfg.key = bidder.clone();
        }
        let key = cfg.key.clone();
        cfg_by_syncer_key
            .entry(key)
            .or_default()
            .push(NamedSyncerConfig {
                name: bidder,
                cfg,
            });
    }

    // Resolve host endpoint
    let mut host_user_sync = host_config.user_sync.clone();
    if host_user_sync.external_url.is_empty() {
        host_user_sync.external_url = host_config.external_url.clone();
    }

    // Create syncers
    let mut errs: Vec<String> = Vec::new();
    let mut syncers: HashMap<String, Syncer> = HashMap::new();

    for (key, cfg_group) in &cfg_by_syncer_key {
        let primary_cfg = match choose_syncer_config(cfg_group) {
            Ok(cfg) => cfg,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };

        for named in cfg_group {
            match new_syncer_from_config(&host_user_sync, &primary_cfg.cfg, &named.name) {
                Ok(syncer) => {
                    syncers.insert(named.name.clone(), syncer);
                }
                Err(e) => {
                    errs.push(
                        SyncerBuildError {
                            bidder: primary_cfg.name.clone(),
                            syncer_key: key.clone(),
                            err: e,
                        }
                        .to_string(),
                    );
                }
            }
        }
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(syncers)
}

/// Determines whether a syncer should be created for a given bidder.
///
/// A syncer is only created when the bidder is enabled and has a meaningful
/// syncer configuration (more than just `supports`).
///
/// Mirrors Go `shouldCreateSyncer()` in `usersync/syncersbuilder.go`.
fn should_create_syncer(info: &BidderInfo) -> bool {
    if !info.is_enabled() {
        return false;
    }
    info.syncer_defined()
}

/// When multiple bidders share the same syncer key, choose which config to
/// use as the primary. At most one bidder in the group may define endpoints
/// (iframe/redirect). If multiple do, their configs must be identical.
///
/// Mirrors Go `chooseSyncerConfig()` in `usersync/syncersbuilder.go`.
fn choose_syncer_config(group: &[NamedSyncerConfig]) -> Result<NamedSyncerConfig, String> {
    if group.len() == 1 {
        return Ok(group[0].clone());
    }

    let mut bidder_names: Vec<&str> = Vec::new();
    let mut with_endpoints: Vec<&NamedSyncerConfig> = Vec::new();

    for named in group {
        bidder_names.push(&named.name);
        if named.cfg.iframe.is_some() || named.cfg.redirect.is_some() {
            with_endpoints.push(named);
        }
    }

    if with_endpoints.is_empty() {
        let mut sorted = bidder_names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        sorted.sort();
        return Err(format!(
            "bidders {} share the same syncer key, but none define endpoints (iframe and/or redirect)",
            sorted.join(", ")
        ));
    }

    if with_endpoints.len() > 1 {
        let first = &with_endpoints[0].cfg;
        for other in &with_endpoints[1..] {
            if *first != other.cfg {
                let mut ep_names: Vec<String> = with_endpoints
                    .iter()
                    .map(|n| n.name.clone())
                    .collect();
                ep_names.sort();
                return Err(format!(
                    "bidders {} define endpoints (iframe and/or redirect) for the same syncer key, but only one bidder is permitted to define endpoints unless the configs are identical",
                    ep_names.join(", ")
                ));
            }
        }
    }

    Ok(with_endpoints[0].clone())
}

/// Construct a `Syncer` from host and bidder syncer configuration.
///
/// Performs macro substitution on the endpoint URLs, replacing template
/// variables like `{{.ExternalURL}}`, `{{.SyncerKey}}`, `{{.BidderName}}`,
/// `{{.SyncType}}`, `{{.UserMacro}}`, and `{{.RedirectURL}}`.
///
/// This is a simplified Rust port of the Go `NewSyncer()` in `usersync/syncer.go`.
fn new_syncer_from_config(
    host_config: &UserSyncConfig,
    syncer_config: &BidderSyncerConfig,
    bidder: &str,
) -> Result<Syncer, String> {
    if syncer_config.key.is_empty() {
        return Err("key is required".to_string());
    }

    if syncer_config.iframe.is_none() && syncer_config.redirect.is_none() {
        return Err(
            "at least one endpoint (iframe and/or redirect) is required".to_string(),
        );
    }

    let iframe_url = if let Some(ref iframe_cfg) = syncer_config.iframe {
        Some(build_syncer_url(
            bidder,
            "i",
            host_config,
            &syncer_config.external_url,
            iframe_cfg,
            &syncer_config.format_override,
        )?)
    } else {
        None
    };

    let redirect_url = if let Some(ref redirect_cfg) = syncer_config.redirect {
        Some(build_syncer_url(
            bidder,
            "b",
            host_config,
            &syncer_config.external_url,
            redirect_cfg,
            &syncer_config.format_override,
        )?)
    } else {
        None
    };

    Ok(Syncer {
        bidder: syncer_config.key.clone(),
        iframe_url,
        redirect_url,
    })
}

/// Build a syncer endpoint URL by performing macro substitution.
///
/// Mirrors Go `buildTemplate()` in `usersync/syncer.go`, but produces a
/// final URL string with privacy macros left as `{gdpr}` / `{gdpr_consent}`
/// placeholders (resolved at sync time by `get_sync_url`).
fn build_syncer_url(
    bidder_name: &str,
    sync_type_value: &str,
    host_config: &UserSyncConfig,
    syncer_external_url: &str,
    endpoint: &SyncerEndpointConfig,
    format_override: &str,
) -> Result<String, String> {
    let redirect_template = if endpoint.redirect_url.is_empty() {
        &host_config.redirect_url
    } else {
        &endpoint.redirect_url
    };

    let effective_sync_type = if format_override.is_empty() {
        sync_type_value
    } else {
        format_override
    };

    let external_url = choose_external_url(
        &endpoint.external_url,
        syncer_external_url,
        &host_config.external_url,
    );

    // Build the redirect URL by substituting macros
    let redirect_url = macro_replace_syncer_key(redirect_template, bidder_name);
    let redirect_url = macro_replace_bidder_name(&redirect_url, bidder_name);
    let redirect_url = macro_replace_sync_type(&redirect_url, effective_sync_type);
    let redirect_url = macro_replace_user_macro(&redirect_url, &endpoint.user_macro);
    let redirect_url = macro_replace_external_url(&redirect_url, &external_url);

    // Substitute the redirect URL into the endpoint URL
    let url = macro_replace_redirect(&endpoint.url, &redirect_url);

    Ok(url)
}

/// Selects the most specific external URL, falling back to less specific ones.
///
/// Mirrors Go `chooseExternalURL()` in `usersync/syncer.go`.
fn choose_external_url<'a>(
    syncer_endpoint_url: &'a str,
    syncer_url: &'a str,
    host_config_url: &'a str,
) -> &'a str {
    if !syncer_endpoint_url.is_empty() {
        return syncer_endpoint_url;
    }
    if !syncer_url.is_empty() {
        return syncer_url;
    }
    host_config_url
}

// Macro replacement helpers using regex.
// These mirror the Go macroRegex* patterns in usersync/syncer.go.

fn macro_replace_external_url(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.ExternalURL\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

fn macro_replace_syncer_key(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.SyncerKey\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

fn macro_replace_bidder_name(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.BidderName\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

fn macro_replace_sync_type(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.SyncType\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

fn macro_replace_user_macro(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.UserMacro\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

fn macro_replace_redirect(s: &str, val: &str) -> String {
    let re = Regex::new(r"\{\{\s*\.RedirectURL\s*\}\}").unwrap();
    re.replace_all(s, val).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_syncer(
        bidder: &str,
        iframe: Option<&str>,
        redirect: Option<&str>,
    ) -> (String, Syncer) {
        (
            bidder.to_string(),
            Syncer {
                bidder: bidder.to_string(),
                iframe_url: iframe.map(String::from),
                redirect_url: redirect.map(String::from),
            },
        )
    }

    #[test]
    fn test_prebid_cookie_round_trip() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("appnexus".to_string(), "uid123".to_string());
        let encoded = cookie.to_cookie_string();
        let decoded = PrebidCookie::from_cookie(&encoded);
        assert_eq!(decoded.get_uid("appnexus").unwrap().uid, "uid123");
    }

    #[test]
    fn test_choose_bidders_opts_out() {
        let cookie = PrebidCookie {
            opt_out: true,
            ..Default::default()
        };
        let syncers = HashMap::new();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert!(result.syncs.is_empty());
        assert_eq!(result.rejected[0].reason, RejectionReason::OptedOut);
    }

    #[test]
    fn test_choose_bidders_already_synced() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("appnexus".to_string(), "uid123".to_string());
        let syncers: HashMap<String, Syncer> =
            [make_syncer("appnexus", Some("http://iframe"), None)]
                .into_iter()
                .collect();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert!(result.syncs.is_empty());
        assert_eq!(result.rejected[0].reason, RejectionReason::AlreadySynced);
    }

    #[test]
    fn test_choose_bidders_success() {
        let cookie = PrebidCookie::new();
        let syncers: HashMap<String, Syncer> = [make_syncer(
            "appnexus",
            Some("http://iframe?gdpr={gdpr}"),
            None,
        )]
        .into_iter()
        .collect();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert_eq!(result.syncs.len(), 1);
        assert_eq!(result.syncs[0].bidder, "appnexus");
        assert!(result.syncs[0].url.contains("gdpr=0"));
    }

    #[test]
    fn test_choose_bidders_max_limit() {
        let cookie = PrebidCookie::new();
        let syncers: HashMap<String, Syncer> = [
            make_syncer("a", Some("http://a"), None),
            make_syncer("b", Some("http://b"), None),
            make_syncer("c", Some("http://c"), None),
        ]
        .into_iter()
        .collect();
        let config = ChooserConfig {
            max_syncs_per_request: 2,
            ..Default::default()
        };
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["a".to_string(), "b".to_string(), "c".to_string()],
            false,
            "",
            &config,
        );
        assert_eq!(result.syncs.len(), 2);
        assert_eq!(
            result
                .rejected
                .iter()
                .filter(|r| r.reason == RejectionReason::MaxSyncsReached)
                .count(),
            1
        );
    }

    #[test]
    fn test_build_sync_response() {
        let cookie = PrebidCookie::new();
        let choose_result = ChooseResult {
            syncs: vec![SyncerChoice {
                bidder: "appnexus".to_string(),
                sync_type: SyncType::Iframe,
                url: "http://sync".to_string(),
            }],
            rejected: vec![],
        };
        let response = build_sync_response(&cookie, &choose_result);
        assert_eq!(response.status, CookieSyncStatus::NoCookie);
        assert_eq!(response.bidder_status.len(), 1);
    }

    // -----------------------------------------------------------------------
    // BidderFilter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_specific_bidder_filter_include() {
        let f = SpecificBidderFilter::new(&["appnexus", "rubicon"], BidderFilterMode::Include);
        assert!(f.allowed("appnexus"));
        assert!(f.allowed("Appnexus")); // case insensitive
        assert!(f.allowed("rubicon"));
        assert!(!f.allowed("pubmatic"));
    }

    #[test]
    fn test_specific_bidder_filter_exclude() {
        let f = SpecificBidderFilter::new(&["appnexus"], BidderFilterMode::Exclude);
        assert!(!f.allowed("appnexus"));
        assert!(f.allowed("rubicon"));
        assert!(f.allowed("pubmatic"));
    }

    #[test]
    fn test_uniform_bidder_filter_include() {
        let f = UniformBidderFilter::new(BidderFilterMode::Include);
        assert!(f.allowed("anything"));
        assert!(f.allowed("appnexus"));
    }

    #[test]
    fn test_uniform_bidder_filter_exclude() {
        let f = UniformBidderFilter::new(BidderFilterMode::Exclude);
        assert!(!f.allowed("anything"));
        assert!(!f.allowed("appnexus"));
    }

    // -----------------------------------------------------------------------
    // SyncTypeFilter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_sync_type_filter_both_allowed() {
        let filter = SyncTypeFilter::new(
            Box::new(UniformBidderFilter::new(BidderFilterMode::Include)),
            Box::new(UniformBidderFilter::new(BidderFilterMode::Include)),
        );
        let types = filter.for_bidder("appnexus");
        assert_eq!(types, vec![SyncType::Iframe, SyncType::Redirect]);
    }

    #[test]
    fn test_sync_type_filter_iframe_only() {
        let filter = SyncTypeFilter::new(
            Box::new(UniformBidderFilter::new(BidderFilterMode::Include)),
            Box::new(UniformBidderFilter::new(BidderFilterMode::Exclude)),
        );
        let types = filter.for_bidder("appnexus");
        assert_eq!(types, vec![SyncType::Iframe]);
    }

    #[test]
    fn test_sync_type_filter_none_allowed() {
        let filter = SyncTypeFilter::new(
            Box::new(UniformBidderFilter::new(BidderFilterMode::Exclude)),
            Box::new(UniformBidderFilter::new(BidderFilterMode::Exclude)),
        );
        let types = filter.for_bidder("appnexus");
        assert!(types.is_empty());
    }

    #[test]
    fn test_sync_type_filter_specific_bidder() {
        let filter = SyncTypeFilter::new(
            Box::new(SpecificBidderFilter::new(
                &["appnexus"],
                BidderFilterMode::Include,
            )),
            Box::new(SpecificBidderFilter::new(
                &["rubicon"],
                BidderFilterMode::Include,
            )),
        );
        assert_eq!(filter.for_bidder("appnexus"), vec![SyncType::Iframe]);
        assert_eq!(filter.for_bidder("rubicon"), vec![SyncType::Redirect]);
        assert!(filter.for_bidder("pubmatic").is_empty());
    }

    // -----------------------------------------------------------------------
    // Ejector tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_oldest_ejector_basic() {
        let mut ejector = OldestEjector;
        let mut uids = HashMap::new();
        uids.insert(
            "a".to_string(),
            UidEntry {
                uid: "u1".to_string(),
                expires: Some("2025-01-01".to_string()),
            },
        );
        uids.insert(
            "b".to_string(),
            UidEntry {
                uid: "u2".to_string(),
                expires: Some("2024-01-01".to_string()),
            },
        );
        uids.insert(
            "c".to_string(),
            UidEntry {
                uid: "u3".to_string(),
                expires: Some("2026-01-01".to_string()),
            },
        );
        let result = ejector.choose(&uids).unwrap();
        assert_eq!(result, "b"); // earliest expiry
    }

    #[test]
    fn test_oldest_ejector_empty() {
        let mut ejector = OldestEjector;
        let uids = HashMap::new();
        assert!(ejector.choose(&uids).is_err());
    }

    #[test]
    fn test_oldest_ejector_no_expiry_treated_as_oldest() {
        let mut ejector = OldestEjector;
        let mut uids = HashMap::new();
        uids.insert(
            "a".to_string(),
            UidEntry {
                uid: "u1".to_string(),
                expires: Some("2025-01-01".to_string()),
            },
        );
        uids.insert(
            "b".to_string(),
            UidEntry {
                uid: "u2".to_string(),
                expires: None,
            },
        );
        let result = ejector.choose(&uids).unwrap();
        assert_eq!(result, "b");
    }

    #[test]
    fn test_priority_ejector_ejects_non_priority_first() {
        let mut syncer_keys: HashMap<String, String> = HashMap::new();
        syncer_keys.insert("bidderA".to_string(), "a".to_string());
        syncer_keys.insert("bidderB".to_string(), "b".to_string());
        syncer_keys.insert("bidderC".to_string(), "c".to_string());

        let mut ejector = PriorityBidderEjector {
            priority_groups: vec![vec!["bidderA".to_string()]],
            syncer_keys_by_bidder: syncer_keys,
            is_syncer_priority: false,
            tie_ejector: Box::new(OldestEjector),
        };

        let mut uids = HashMap::new();
        uids.insert(
            "a".to_string(),
            UidEntry {
                uid: "u1".to_string(),
                expires: Some("2024-01-01".to_string()),
            },
        );
        uids.insert(
            "b".to_string(),
            UidEntry {
                uid: "u2".to_string(),
                expires: Some("2025-01-01".to_string()),
            },
        );
        uids.insert(
            "c".to_string(),
            UidEntry {
                uid: "u3".to_string(),
                expires: Some("2024-06-01".to_string()),
            },
        );

        // "b" and "c" are non-priority; "c" has the earlier expiry among non-priority
        let result = ejector.choose(&uids).unwrap();
        assert_eq!(result, "c");
    }

    #[test]
    fn test_priority_ejector_single_lowest_group() {
        let mut syncer_keys: HashMap<String, String> = HashMap::new();
        syncer_keys.insert("bidderA".to_string(), "a".to_string());
        syncer_keys.insert("bidderB".to_string(), "b".to_string());

        let mut ejector = PriorityBidderEjector {
            priority_groups: vec![
                vec!["bidderA".to_string()],
                vec!["bidderB".to_string()],
            ],
            syncer_keys_by_bidder: syncer_keys,
            is_syncer_priority: true,
            tie_ejector: Box::new(OldestEjector),
        };

        let mut uids = HashMap::new();
        uids.insert(
            "a".to_string(),
            UidEntry {
                uid: "u1".to_string(),
                expires: Some("2024-01-01".to_string()),
            },
        );
        uids.insert(
            "b".to_string(),
            UidEntry {
                uid: "u2".to_string(),
                expires: Some("2025-01-01".to_string()),
            },
        );

        // All are priority; lowest group is ["bidderB"], single element -> eject it
        let result = ejector.choose(&uids).unwrap();
        assert_eq!(result, "bidderB");
        // The lowest group should have been removed
        assert_eq!(ejector.priority_groups.len(), 1);
    }

    #[test]
    fn test_priority_ejector_error_when_not_priority_and_only_priority_remain() {
        let mut syncer_keys: HashMap<String, String> = HashMap::new();
        syncer_keys.insert("bidderA".to_string(), "a".to_string());

        let mut ejector = PriorityBidderEjector {
            priority_groups: vec![vec!["bidderA".to_string()]],
            syncer_keys_by_bidder: syncer_keys,
            is_syncer_priority: false,
            tie_ejector: Box::new(OldestEjector),
        };

        let mut uids = HashMap::new();
        // One non-priority uid
        uids.insert(
            "x".to_string(),
            UidEntry {
                uid: "u1".to_string(),
                expires: Some("2024-01-01".to_string()),
            },
        );
        uids.insert(
            "a".to_string(),
            UidEntry {
                uid: "u2".to_string(),
                expires: Some("2025-01-01".to_string()),
            },
        );

        // non_priority has 1 element, is_syncer_priority is false, priority_groups non-empty -> error
        let result = ejector.choose(&uids);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("syncer key is not a priority"));
    }

    // -----------------------------------------------------------------------
    // Cookie encoder / decoder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_encode_decode_cookie_round_trip() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("appnexus".to_string(), "uid123".to_string());
        cookie.set_uid("rubicon".to_string(), "uid456".to_string());

        let encoded = encode_cookie(&cookie).unwrap();
        let decoded = decode_cookie(&encoded);

        assert_eq!(decoded.get_uid("appnexus").unwrap().uid, "uid123");
        assert_eq!(decoded.get_uid("rubicon").unwrap().uid, "uid456");
        assert!(!decoded.opt_out);
    }

    #[test]
    fn test_decode_cookie_invalid_base64() {
        let decoded = decode_cookie("not-valid-base64!!!");
        assert!(decoded.uids.is_empty());
        assert!(!decoded.opt_out);
    }

    #[test]
    fn test_decode_cookie_invalid_json() {
        use base64::Engine;
        let encoded =
            base64::engine::general_purpose::URL_SAFE.encode(b"this is not json");
        let decoded = decode_cookie(&encoded);
        assert!(decoded.uids.is_empty());
    }

    #[test]
    fn test_encode_cookie_empty() {
        let cookie = PrebidCookie::new();
        let encoded = encode_cookie(&cookie).unwrap();
        assert!(!encoded.is_empty());
        let decoded = decode_cookie(&encoded);
        assert!(decoded.uids.is_empty());
    }

    #[test]
    fn test_encode_cookie_with_opt_out() {
        let cookie = PrebidCookie {
            opt_out: true,
            ..Default::default()
        };
        let encoded = encode_cookie(&cookie).unwrap();
        let decoded = decode_cookie(&encoded);
        assert!(decoded.opt_out);
    }

    // -----------------------------------------------------------------------
    // BidderChooser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_choose_bidder_order_not_cooperative_with_requested() {
        let requested = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let available = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()];
        let coop = Cooperative {
            enabled: false,
            priority_groups: vec![],
        };
        let result = choose_bidder_order(&requested, &available, &coop);
        // Should contain only requested bidders (shuffled)
        assert_eq!(result.len(), 3);
        let mut sorted = result.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_choose_bidder_order_not_cooperative_no_requested() {
        let requested: Vec<String> = vec![];
        let available = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let coop = Cooperative {
            enabled: false,
            priority_groups: vec![],
        };
        let result = choose_bidder_order(&requested, &available, &coop);
        // Should contain all available bidders (shuffled)
        assert_eq!(result.len(), 3);
        let mut sorted = result.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_choose_bidder_order_cooperative() {
        let requested = vec!["x".to_string()];
        let available = vec!["a".to_string(), "b".to_string()];
        let coop = Cooperative {
            enabled: true,
            priority_groups: vec![vec!["p1".to_string(), "p2".to_string()]],
        };
        let result = choose_bidder_order(&requested, &available, &coop);
        // Should contain: requested + priority group + available (with duplicates possible)
        assert_eq!(result.len(), 1 + 2 + 2); // 5 total
        // First element must be "x" (only requested bidder)
        assert_eq!(result[0], "x");
        // The priority group elements should appear at indices 1-2
        let mut priority_part: Vec<String> = result[1..3].to_vec();
        priority_part.sort();
        assert_eq!(priority_part, vec!["p1", "p2"]);
        // The available elements should appear at indices 3-4
        let mut avail_part: Vec<String> = result[3..5].to_vec();
        avail_part.sort();
        assert_eq!(avail_part, vec!["a", "b"]);
    }

    #[test]
    fn test_choose_bidder_order_cooperative_empty_requested() {
        let requested: Vec<String> = vec![];
        let available = vec!["a".to_string(), "b".to_string()];
        let coop = Cooperative {
            enabled: true,
            priority_groups: vec![],
        };
        let result = choose_bidder_order(&requested, &available, &coop);
        // Should contain just available (shuffled)
        assert_eq!(result.len(), 2);
        let mut sorted = result.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b"]);
    }

    #[test]
    fn test_choose_bidder_order_cooperative_multiple_priority_groups() {
        let requested: Vec<String> = vec![];
        let available = vec!["z".to_string()];
        let coop = Cooperative {
            enabled: true,
            priority_groups: vec![
                vec!["g1a".to_string(), "g1b".to_string()],
                vec!["g2a".to_string()],
            ],
        };
        let result = choose_bidder_order(&requested, &available, &coop);
        // 0 requested + 2 (group1) + 1 (group2) + 1 (available) = 4
        assert_eq!(result.len(), 4);
    }

    // -----------------------------------------------------------------------
    // Shuffler tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_random_shuffler_preserves_elements() {
        let shuffler = RandomShuffler;
        let mut items: Vec<String> = vec!["a", "b", "c", "d", "e"]
            .into_iter()
            .map(String::from)
            .collect();
        shuffler.shuffle(&mut items);
        items.sort();
        assert_eq!(items, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn test_random_shuffler_empty() {
        let shuffler = RandomShuffler;
        let mut items: Vec<String> = vec![];
        shuffler.shuffle(&mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn test_random_shuffler_single() {
        let shuffler = RandomShuffler;
        let mut items = vec!["only".to_string()];
        shuffler.shuffle(&mut items);
        assert_eq!(items, vec!["only"]);
    }

    // -----------------------------------------------------------------------
    // Syncer method tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_syncer_key() {
        let (_, syncer) = make_syncer("appnexus", Some("http://iframe"), None);
        assert_eq!(syncer.key(), "appnexus");
    }

    #[test]
    fn test_syncer_default_response_format_iframe() {
        let (_, syncer) = make_syncer("appnexus", Some("http://iframe"), None);
        assert_eq!(syncer.default_response_format(), SyncType::Iframe);
    }

    #[test]
    fn test_syncer_default_response_format_redirect() {
        let (_, syncer) = make_syncer("appnexus", None, Some("http://redirect"));
        assert_eq!(syncer.default_response_format(), SyncType::Redirect);
    }

    #[test]
    fn test_syncer_supports_type_iframe() {
        let (_, syncer) = make_syncer("appnexus", Some("http://iframe"), None);
        assert!(syncer.supports_type(&[SyncType::Iframe]));
        assert!(!syncer.supports_type(&[SyncType::Redirect]));
    }

    #[test]
    fn test_syncer_supports_type_both() {
        let (_, syncer) =
            make_syncer("appnexus", Some("http://iframe"), Some("http://redirect"));
        assert!(syncer.supports_type(&[SyncType::Iframe]));
        assert!(syncer.supports_type(&[SyncType::Redirect]));
        assert!(syncer.supports_type(&[SyncType::Iframe, SyncType::Redirect]));
    }

    #[test]
    fn test_syncer_supports_type_empty() {
        let (_, syncer) = make_syncer("appnexus", Some("http://iframe"), None);
        assert!(!syncer.supports_type(&[]));
    }

    // -----------------------------------------------------------------------
    // SyncerBuilder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_should_create_syncer_disabled() {
        let info = BidderInfo {
            disabled: true,
            user_sync: Some(BidderSyncerConfig {
                key: "test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!should_create_syncer(&info));
    }

    #[test]
    fn test_should_create_syncer_no_syncer_config() {
        let info = BidderInfo {
            user_sync: None,
            ..Default::default()
        };
        assert!(!should_create_syncer(&info));
    }

    #[test]
    fn test_should_create_syncer_empty_syncer_config() {
        let info = BidderInfo {
            user_sync: Some(BidderSyncerConfig::default()),
            ..Default::default()
        };
        assert!(!should_create_syncer(&info));
    }

    #[test]
    fn test_should_create_syncer_with_key() {
        let info = BidderInfo {
            user_sync: Some(BidderSyncerConfig {
                key: "mykey".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(should_create_syncer(&info));
    }

    #[test]
    fn test_choose_syncer_config_single() {
        let group = vec![NamedSyncerConfig {
            name: "bidderA".to_string(),
            cfg: BidderSyncerConfig {
                key: "a".to_string(),
                iframe: Some(SyncerEndpointConfig {
                    url: "http://example.com".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        }];
        let result = choose_syncer_config(&group);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "bidderA");
    }

    #[test]
    fn test_choose_syncer_config_no_endpoints() {
        let group = vec![
            NamedSyncerConfig {
                name: "a".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    ..Default::default()
                },
            },
            NamedSyncerConfig {
                name: "b".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    ..Default::default()
                },
            },
        ];
        let result = choose_syncer_config(&group);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("none define endpoints"));
    }

    #[test]
    fn test_choose_syncer_config_multiple_different_endpoints() {
        let group = vec![
            NamedSyncerConfig {
                name: "a".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    iframe: Some(SyncerEndpointConfig {
                        url: "http://a.com".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            },
            NamedSyncerConfig {
                name: "b".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    iframe: Some(SyncerEndpointConfig {
                        url: "http://b.com".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            },
        ];
        let result = choose_syncer_config(&group);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("only one bidder is permitted"));
    }

    #[test]
    fn test_choose_syncer_config_multiple_identical_endpoints() {
        let endpoint = SyncerEndpointConfig {
            url: "http://same.com".to_string(),
            ..Default::default()
        };
        let group = vec![
            NamedSyncerConfig {
                name: "a".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    iframe: Some(endpoint.clone()),
                    ..Default::default()
                },
            },
            NamedSyncerConfig {
                name: "b".to_string(),
                cfg: BidderSyncerConfig {
                    key: "shared".to_string(),
                    iframe: Some(endpoint),
                    ..Default::default()
                },
            },
        ];
        let result = choose_syncer_config(&group);
        assert!(result.is_ok());
    }

    #[test]
    fn test_new_syncer_from_config_missing_key() {
        let host = UserSyncConfig::default();
        let cfg = BidderSyncerConfig {
            key: "".to_string(),
            iframe: Some(SyncerEndpointConfig::default()),
            ..Default::default()
        };
        let result = new_syncer_from_config(&host, &cfg, "bidder");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("key is required"));
    }

    #[test]
    fn test_new_syncer_from_config_no_endpoints() {
        let host = UserSyncConfig::default();
        let cfg = BidderSyncerConfig {
            key: "mykey".to_string(),
            ..Default::default()
        };
        let result = new_syncer_from_config(&host, &cfg, "bidder");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("at least one endpoint"));
    }

    #[test]
    fn test_new_syncer_from_config_iframe_only() {
        let host = UserSyncConfig {
            external_url: "http://host.com".to_string(),
            redirect_url: "http://host.com/setuid?bidder={{.SyncerKey}}&uid={{.UserMacro}}"
                .to_string(),
            ..Default::default()
        };
        let cfg = BidderSyncerConfig {
            key: "appnexus".to_string(),
            iframe: Some(SyncerEndpointConfig {
                url: "http://ib.adnxs.com/getuid?{{.RedirectURL}}".to_string(),
                user_macro: "$UID".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let syncer = new_syncer_from_config(&host, &cfg, "appnexus").unwrap();
        assert_eq!(syncer.key(), "appnexus");
        assert!(syncer.iframe_url.is_some());
        assert!(syncer.redirect_url.is_none());
        let iframe = syncer.iframe_url.unwrap();
        assert!(iframe.contains("ib.adnxs.com"));
    }

    #[test]
    fn test_new_syncer_from_config_both_endpoints() {
        let host = UserSyncConfig {
            external_url: "http://host.com".to_string(),
            redirect_url: "http://host.com/setuid?bidder={{.SyncerKey}}".to_string(),
            ..Default::default()
        };
        let cfg = BidderSyncerConfig {
            key: "rubicon".to_string(),
            iframe: Some(SyncerEndpointConfig {
                url: "http://iframe.example.com".to_string(),
                ..Default::default()
            }),
            redirect: Some(SyncerEndpointConfig {
                url: "http://redirect.example.com".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let syncer = new_syncer_from_config(&host, &cfg, "rubicon").unwrap();
        assert!(syncer.iframe_url.is_some());
        assert!(syncer.redirect_url.is_some());
    }

    #[test]
    fn test_macro_replacement() {
        let input = "http://example.com?key={{ .SyncerKey }}&type={{ .SyncType }}";
        let result = macro_replace_syncer_key(input, "appnexus");
        assert!(result.contains("key=appnexus"));
        let result = macro_replace_sync_type(&result, "i");
        assert!(result.contains("type=i"));
    }

    #[test]
    fn test_choose_external_url_endpoint_wins() {
        assert_eq!(
            choose_external_url("http://ep.com", "http://syncer.com", "http://host.com"),
            "http://ep.com"
        );
    }

    #[test]
    fn test_choose_external_url_syncer_wins() {
        assert_eq!(
            choose_external_url("", "http://syncer.com", "http://host.com"),
            "http://syncer.com"
        );
    }

    #[test]
    fn test_choose_external_url_host_fallback() {
        assert_eq!(
            choose_external_url("", "", "http://host.com"),
            "http://host.com"
        );
    }

    #[test]
    fn test_build_syncers_success() {
        let host = Configuration {
            external_url: "http://host.com".to_string(),
            user_sync: UserSyncConfig {
                redirect_url: "http://host.com/setuid?bidder={{.SyncerKey}}".to_string(),
                external_url: "http://host.com".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bidder_infos = HashMap::new();
        bidder_infos.insert(
            "appnexus".to_string(),
            BidderInfo {
                user_sync: Some(BidderSyncerConfig {
                    key: "appnexus".to_string(),
                    iframe: Some(SyncerEndpointConfig {
                        url: "http://ib.adnxs.com/getuid".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let result = build_syncers(&host, &bidder_infos);
        assert!(result.is_ok());
        let syncers = result.unwrap();
        assert!(syncers.contains_key("appnexus"));
    }

    #[test]
    fn test_build_syncers_disabled_bidder_skipped() {
        let host = Configuration {
            external_url: "http://host.com".to_string(),
            ..Default::default()
        };
        let mut bidder_infos = HashMap::new();
        bidder_infos.insert(
            "disabled_bidder".to_string(),
            BidderInfo {
                disabled: true,
                user_sync: Some(BidderSyncerConfig {
                    key: "disabled".to_string(),
                    iframe: Some(SyncerEndpointConfig {
                        url: "http://example.com".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let result = build_syncers(&host, &bidder_infos);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_build_syncers_default_key() {
        // When key is empty, bidder name is used as key
        let host = Configuration {
            external_url: "http://host.com".to_string(),
            user_sync: UserSyncConfig {
                redirect_url: "http://host.com/setuid".to_string(),
                external_url: "http://host.com".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bidder_infos = HashMap::new();
        bidder_infos.insert(
            "mybidder".to_string(),
            BidderInfo {
                user_sync: Some(BidderSyncerConfig {
                    key: "".to_string(), // empty key
                    iframe: Some(SyncerEndpointConfig {
                        url: "http://example.com".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let result = build_syncers(&host, &bidder_infos);
        assert!(result.is_ok());
        let syncers = result.unwrap();
        assert!(syncers.contains_key("mybidder"));
        // The syncer key should be the bidder name
        assert_eq!(syncers["mybidder"].key(), "mybidder");
    }

    #[test]
    fn test_syncer_build_error_display() {
        let err = SyncerBuildError {
            bidder: "appnexus".to_string(),
            syncer_key: "appnexus".to_string(),
            err: "key is required".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("appnexus"));
        assert!(msg.contains("key is required"));
    }
}
