//! Bidder selection for user syncs.
//!
//! Ported from `usersync/chooser.go` (simplified filtering-only form).

use crate::cookie::Cookie;
use crate::syncer::{Syncer, SyncTypeFilter};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Result status of evaluating a bidder for syncing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    BlockedByUserOptOut,
    AlreadySynced,
    UnknownBidder,
    RejectedByFilter,
    Duplicate,
    BlockedByPrivacy,
}

/// Evaluation record for a single bidder considered for syncing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidderEvaluation {
    pub bidder: String,
    pub syncer_key: String,
    pub status: Status,
}

/// A syncer chosen for a bidder.
#[derive(Clone)]
pub struct SyncerChoice {
    pub bidder: String,
    pub syncer: Arc<dyn Syncer>,
}

impl std::fmt::Debug for SyncerChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncerChoice")
            .field("bidder", &self.bidder)
            .field("syncer_key", &self.syncer.key())
            .finish()
    }
}

/// Request describing a user sync selection call.
#[derive(Debug, Clone, Default)]
pub struct ChooserRequest {
    /// Bidders requested by the caller.
    pub bidders: Vec<String>,
    /// Optional cap on the number of chosen bidders (0 means unlimited).
    pub limit: usize,
    /// Acceptable sync types.
    pub sync_type_filter: SyncTypeFilter,
    /// Simple privacy gate: bidders in this set are blocked.
    pub privacy_blocked: HashSet<String>,
}

/// Top-level result of a chooser invocation.
#[derive(Debug, Clone)]
pub struct ChooserResult {
    pub bidders_evaluated: Vec<BidderEvaluation>,
    pub syncers_chosen: Vec<SyncerChoice>,
    pub status: Status,
}

/// Decides which bidders should be synced for a given request and cookie.
pub trait Chooser: Send + Sync {
    fn choose(&self, request: &ChooserRequest, cookie: &Cookie) -> ChooserResult;
}

/// A simple filtering-only [`Chooser`] implementation that walks the requested
/// bidders in order and applies cookie/privacy/filter checks.
pub struct StandardChooser {
    bidder_syncer_lookup: HashMap<String, Arc<dyn Syncer>>,
}

impl StandardChooser {
    pub fn new(bidder_syncer_lookup: HashMap<String, Arc<dyn Syncer>>) -> Self {
        Self {
            bidder_syncer_lookup,
        }
    }

    fn evaluate(
        &self,
        bidder: &str,
        syncers_seen: &mut HashSet<String>,
        request: &ChooserRequest,
        cookie: &Cookie,
    ) -> (Option<Arc<dyn Syncer>>, BidderEvaluation) {
        let Some(syncer) = self.bidder_syncer_lookup.get(bidder) else {
            return (
                None,
                BidderEvaluation {
                    bidder: bidder.to_string(),
                    syncer_key: String::new(),
                    status: Status::UnknownBidder,
                },
            );
        };

        let key = syncer.key().to_string();
        if !syncers_seen.insert(key.clone()) {
            return (
                None,
                BidderEvaluation {
                    bidder: bidder.to_string(),
                    syncer_key: key,
                    status: Status::Duplicate,
                },
            );
        }

        if !syncer.supports_type(&request.sync_type_filter.allowed) {
            return (
                None,
                BidderEvaluation {
                    bidder: bidder.to_string(),
                    syncer_key: key,
                    status: Status::RejectedByFilter,
                },
            );
        }

        if cookie.has_live_sync(&key) {
            return (
                None,
                BidderEvaluation {
                    bidder: bidder.to_string(),
                    syncer_key: key,
                    status: Status::AlreadySynced,
                },
            );
        }

        if request.privacy_blocked.contains(bidder) {
            return (
                None,
                BidderEvaluation {
                    bidder: bidder.to_string(),
                    syncer_key: key,
                    status: Status::BlockedByPrivacy,
                },
            );
        }

        (
            Some(syncer.clone()),
            BidderEvaluation {
                bidder: bidder.to_string(),
                syncer_key: key,
                status: Status::Ok,
            },
        )
    }
}

impl Chooser for StandardChooser {
    fn choose(&self, request: &ChooserRequest, cookie: &Cookie) -> ChooserResult {
        if !cookie.allow_syncs() {
            return ChooserResult {
                bidders_evaluated: vec![],
                syncers_chosen: vec![],
                status: Status::BlockedByUserOptOut,
            };
        }

        let mut syncers_seen = HashSet::new();
        let mut bidders_seen = HashSet::new();
        let mut evaluated = Vec::new();
        let mut chosen = Vec::new();
        let limit_disabled = request.limit == 0;

        for bidder in &request.bidders {
            if !limit_disabled && chosen.len() >= request.limit {
                break;
            }
            if !bidders_seen.insert(bidder.clone()) {
                continue;
            }
            let (syncer_opt, eval) = self.evaluate(bidder, &mut syncers_seen, request, cookie);
            if let Some(syncer) = syncer_opt {
                chosen.push(SyncerChoice {
                    bidder: bidder.clone(),
                    syncer,
                });
            }
            evaluated.push(eval);
        }

        ChooserResult {
            bidders_evaluated: evaluated,
            syncers_chosen: chosen,
            status: Status::Ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syncer::{StandardSyncer, SyncType, SyncTypeFilter};

    fn make_syncer(key: &str, sync_type: SyncType) -> Arc<dyn Syncer> {
        Arc::new(StandardSyncer::new(
            key,
            sync_type,
            Some(format!("https://iframe/{key}")),
            Some(format!("https://redirect/{key}")),
        ))
    }

    fn make_chooser() -> StandardChooser {
        let mut lookup: HashMap<String, Arc<dyn Syncer>> = HashMap::new();
        lookup.insert("rubicon".into(), make_syncer("rubicon", SyncType::Redirect));
        lookup.insert(
            "appnexus".into(),
            make_syncer("appnexus", SyncType::Iframe),
        );
        StandardChooser::new(lookup)
    }

    #[test]
    fn chooses_requested_bidders() {
        let chooser = make_chooser();
        let req = ChooserRequest {
            bidders: vec!["rubicon".into(), "appnexus".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &Cookie::new());
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.syncers_chosen.len(), 2);
        assert_eq!(result.bidders_evaluated.len(), 2);
        assert!(result
            .bidders_evaluated
            .iter()
            .all(|e| e.status == Status::Ok));
    }

    #[test]
    fn opted_out_cookie_blocks_everything() {
        let chooser = make_chooser();
        let mut cookie = Cookie::new();
        cookie.set_opt_out(true);
        let req = ChooserRequest {
            bidders: vec!["rubicon".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &cookie);
        assert_eq!(result.status, Status::BlockedByUserOptOut);
        assert!(result.syncers_chosen.is_empty());
    }

    #[test]
    fn already_synced_is_rejected() {
        let chooser = make_chooser();
        let mut cookie = Cookie::new();
        cookie.sync("rubicon", "abc").unwrap();
        let req = ChooserRequest {
            bidders: vec!["rubicon".into(), "appnexus".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &cookie);
        assert_eq!(result.syncers_chosen.len(), 1);
        assert_eq!(result.syncers_chosen[0].bidder, "appnexus");
        let rubicon_eval = result
            .bidders_evaluated
            .iter()
            .find(|e| e.bidder == "rubicon")
            .unwrap();
        assert_eq!(rubicon_eval.status, Status::AlreadySynced);
    }

    #[test]
    fn unknown_bidder_returns_unknown_status() {
        let chooser = make_chooser();
        let req = ChooserRequest {
            bidders: vec!["unknown".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &Cookie::new());
        assert_eq!(result.bidders_evaluated[0].status, Status::UnknownBidder);
        assert!(result.syncers_chosen.is_empty());
    }

    #[test]
    fn filter_rejects_syncer_without_matching_type() {
        let mut lookup: HashMap<String, Arc<dyn Syncer>> = HashMap::new();
        lookup.insert(
            "iframe-only".into(),
            Arc::new(StandardSyncer::new(
                "iframe-only",
                SyncType::Iframe,
                Some("https://i".into()),
                None,
            )),
        );
        let chooser = StandardChooser::new(lookup);
        let req = ChooserRequest {
            bidders: vec!["iframe-only".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::redirect_only(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &Cookie::new());
        assert_eq!(
            result.bidders_evaluated[0].status,
            Status::RejectedByFilter
        );
    }

    #[test]
    fn privacy_blocked_bidder_is_rejected() {
        let chooser = make_chooser();
        let mut blocked = HashSet::new();
        blocked.insert("rubicon".to_string());
        let req = ChooserRequest {
            bidders: vec!["rubicon".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: blocked,
        };
        let result = chooser.choose(&req, &Cookie::new());
        assert_eq!(
            result.bidders_evaluated[0].status,
            Status::BlockedByPrivacy
        );
    }

    #[test]
    fn limit_caps_chosen_syncers() {
        let chooser = make_chooser();
        let req = ChooserRequest {
            bidders: vec!["rubicon".into(), "appnexus".into()],
            limit: 1,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &Cookie::new());
        assert_eq!(result.syncers_chosen.len(), 1);
    }

    #[test]
    fn duplicate_bidder_is_skipped() {
        let chooser = make_chooser();
        let req = ChooserRequest {
            bidders: vec!["rubicon".into(), "rubicon".into()],
            limit: 0,
            sync_type_filter: SyncTypeFilter::all(),
            privacy_blocked: HashSet::new(),
        };
        let result = chooser.choose(&req, &Cookie::new());
        // Second occurrence should be skipped silently.
        assert_eq!(result.bidders_evaluated.len(), 1);
    }
}
