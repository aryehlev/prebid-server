//! Unit tests for the integration bridges.
//!
//! These tests exercise each bridge with the minimal dependency possible:
//! a `NullAnalyticsBackend` for analytics, a `NullMetrics` engine for
//! metrics, an empty `HookExecutionPlan` for hooks, and a single-leaf
//! `Tree` for rules. Nothing here talks to a real auction pipeline — the
//! goal is just to prove the wiring compiles and runs end-to-end.

use std::sync::Arc;
use std::time::Duration;

use analytics::{AnalyticsRunner, NullAnalyticsBackend};
use hooks::plan::HookExecutionPlan;
use hooks::stage::Stage;
use hooks::HookExecutor;
use pbs_metrics::{NoopMetrics, NullMetrics};
use rules::result_functions::{BidderCtx, IncludeBidders};
use rules::schema_functions::RequestCtx;
use rules::{Node, Rules, Tree};
use serde_json::json;

use super::analytics_bridge::{
    emit_amp_event, emit_auction_event, emit_cookie_sync_event, emit_notification_event,
    emit_setuid_event,
};
use super::hooks_bridge::AuctionHookRunner;
use super::metrics_bridge::{
    record_adapter_call, record_auction_duration, record_auction_request, record_cache_op,
};
use super::rules_bridge::evaluate_request_rules;

fn runner_with_null_backend() -> AnalyticsRunner {
    AnalyticsRunner::with_modules(vec![Arc::new(NullAnalyticsBackend::new())])
        .with_timeout(Duration::from_secs(1))
}

#[tokio::test]
async fn emit_auction_event_fans_out_to_null_backend() {
    let runner = runner_with_null_backend();
    let req = json!({"id": "req-1"});
    let resp = json!({"seatbid": []});
    // None of these should panic.
    emit_auction_event(&runner, &req, &resp, 200, "req-1", "acct-1").await;
}

#[tokio::test]
async fn emit_all_event_kinds_to_null_backend() {
    let runner = runner_with_null_backend();
    let req = json!({"id": "req-1"});
    let resp = json!({"ok": true});
    emit_amp_event(&runner, &req, &resp, 200, "r", "a").await;
    emit_cookie_sync_event(&runner, &req, &resp, 200, "r", "a").await;
    emit_setuid_event(&runner, &req, &resp, 200, "r", "a").await;
    emit_notification_event(&runner, &req, &resp, 200, "r", "a").await;
}

#[test]
fn metrics_bridge_increments_null_metrics() {
    let m: NullMetrics = NoopMetrics;
    // Each of these is a no-op on NullMetrics but must not panic.
    record_auction_request(&m, "/openrtb2/auction", 200);
    record_auction_request(&m, "/openrtb2/amp", 400);
    record_auction_duration(&m, "/openrtb2/auction", 123);
    record_adapter_call(&m, "appnexus", true);
    record_adapter_call(&m, "rubicon", false);
    record_cache_op(&m, "put", true);
    record_cache_op(&m, "get", false);
}

#[tokio::test]
async fn auction_hook_runner_processes_empty_plan_as_noop() {
    let executor = HookExecutor::new(Stage::EntrypointStage)
        .with_account_id("acct-1")
        .with_endpoint("/openrtb2/auction");
    let plan: HookExecutionPlan<i64> = HookExecutionPlan::new();
    let runner = AuctionHookRunner::new(executor, plan);

    assert!(runner.plan().is_empty());
    let outcome = runner.run(42_i64).await;
    assert_eq!(outcome.payload, 42);
    assert!(outcome.error.is_none());
    assert_eq!(outcome.summary.successful_hooks, 0);
    assert_eq!(outcome.summary.mutations_applied, 0);
    assert!(!outcome.summary.rejected);
}

#[test]
fn rules_bridge_evaluates_trivial_single_leaf_tree() {
    // Single-leaf tree: the root itself is a leaf with one include-bidders
    // result function. This exercises the shortest possible evaluation path
    // through `Tree::run` (no schema functions involved).
    let mut root: Node<RequestCtx, BidderCtx> = Node::new();
    root.result_functions.push(Box::new(IncludeBidders {
        bidders: vec!["rubicon".to_string()],
    }));

    let tree = Tree {
        root: Some(root),
        analytics_key: "test".into(),
        model_version: "v1".into(),
        ..Default::default()
    };
    tree.validate().expect("trivial tree is balanced");

    let rules = Rules::new(tree);
    let ctx = RequestCtx {
        device_country: "US".into(),
        channel: "web".into(),
        device_type: "phone".into(),
    };
    let (out, _meta) = evaluate_request_rules(&rules, &ctx).expect("rules evaluate");
    assert_eq!(out.included_bidders, vec!["rubicon".to_string()]);
    assert!(out.excluded_bidders.is_empty());
}
