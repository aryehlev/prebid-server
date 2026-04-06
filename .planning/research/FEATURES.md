# Feature Landscape: Rust Prebid Server Production-Replacement Parity

**Domain:** Production-replacement parity for Prebid Server Go -> Rust
**Researched:** 2026-04-05
**Overall confidence:** HIGH for table-stakes buckets, MEDIUM for defer/anti-feature prioritization

## Framing

For this project, "table stakes" does not mean "nice to have for a competitive bidder host." It means "required before an operator can cut production traffic from Go Prebid Server to Rust without changing request shape, account config, deployment assumptions, or incident playbooks."

Because the explicit project goal is full parity with the existing Go server, anything that changes semantics, narrows the supported surface, or asks operators to reconfigure integrations is not a shortcut. It is a cutover risk.

## Table Stakes For Production Replacement

Features users and operators already expect from Go PBS. Missing any of these means the Rust port is not a credible drop-in replacement.

| Capability Bucket | Maps To | Why Expected | Complexity | Notes |
|---------|---------|---------|---------|---------|
| Public endpoint parity | Auction flow, config/ops compatibility | Existing clients expect the current Go HTTP surface: `/openrtb2/auction`, `/openrtb2/amp`, `/openrtb2/video`, `/cookie_sync`, `/setuid`, `/getuids`, `/status`, `/info/*`, `/event`, `/vtrack`. | High | Response formats, status codes, warnings, and debug fields must remain compatible, not just route names. |
| Core auction request lifecycle parity | Auction flow | The main replacement test is whether the Rust server accepts the same OpenRTB requests, resolves the same stored artifacts, applies the same validation/privacy/hooks, and emits materially identical bid responses. | High | Includes request correction, stored request merge behavior, account resolution, error accumulation, and response shaping. |
| Auction policy features used on the hot path | Auction flow | Production traffic depends on current Go behavior for price granularity, bid adjustments, price floors, currency conversion, targeting, aliases, multibid, request IDs, and debug/trace behavior. | High | These are not optional embellishments; they directly affect monetization, targeting keys, and troubleshooting. |
| Media-type parity | Auction flow, bidders/adapters | Go PBS already supports banner, video, native, audio, multiformat, interstitial, AMP, and long-form video flows. Hosts expect those paths to keep working. | High | Video and multiformat parity also depend on cache, events, and adapter capability metadata. |
| Stored request and stored response parity | Auction flow, caching, config/ops compatibility | Stored requests and stored responses are standard operating infrastructure for many PBS deployments and are explicitly part of the current architecture. | High | Must preserve account scoping, merge precedence, unique bid request ID behavior, and multi-backend semantics. |
| Prebid Cache parity | Caching | Bids, VAST, winning-only caching, application storage, response TTL behavior, and video/event workflows depend on PBC compatibility. | High | This is a top-level active project requirement and a cutover blocker for video, SDK, and event-driven integrations. |
| Full bidder and adapter parity | Bidders/adapters | A production replacement cannot ship with "most" bidders. Go exposes hundreds of bidder metadata files and a very large adapter surface; supported bidders are part of the external product contract. | High | Includes request building, response parsing, aliasing, bidder params validation, syncer metadata, media-type capability declarations, and bidder info endpoints. |
| User sync and identity parity | Bidders/adapters, auction flow | `/cookie_sync`, `/setuid`, `/getuids`, cooperative sync, account overrides, and EID permission behavior are documented PBS features and part of host integrations. | High | Sync behavior is operationally visible and easy for publishers to detect when broken. |
| Privacy and regulatory enforcement parity | Auction flow, modules | GDPR TCF 2.x, USP/CCPA, GPP, COPPA, GPC, DSA passthrough, device LMT, activity controls, and related request scrubbing are table stakes for a modern production host. | High | Operators will not accept a parity server that is less compliant or differently compliant than Go. |
| Module and hook execution parity | Modules | The Go codebase already treats modules/hooks as part of the runtime contract. Execution plan semantics, stage ordering, timeouts, account-vs-host config, and module-specific side effects must match. | High | At minimum this includes the currently shipped module families and hook stages used by Go. |
| Config and bootstrap compatibility | Config/ops compatibility | Existing deployments expect the same config shape, bidder metadata loading, default request behavior, static assets, account defaults, and startup wiring. | High | "Same behavior, new config model" is not parity. Avoid requiring operators to translate configs. |
| Observability and operations compatibility | Config/ops compatibility | Health/status, bidder info, metrics, analytics hooks, request logging, admin behaviors, and version metadata are part of production operations, not extras. | Medium | Exact metric names and admin surfaces matter less than preserving operator workflows, but large drift still blocks cutover confidence. |
| Test parity and behavioral proof | Test parity | Without regression evidence against Go behavior, the project cannot credibly claim production replacement. | High | Requires fixture or golden coverage for endpoints, adapters, stored requests, cache behavior, privacy, hooks/modules, and failure cases. |

## Differentiators To Defer

These may become valuable after cutover, but they should not compete with parity work.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Rust-native performance tuning beyond Go behavior | Better latency and resource efficiency after parity is proven. | High | Good post-parity work; dangerous if it changes timing-sensitive behavior before cutover. |
| New module model or WASM/plugin story | Stronger long-term extensibility for Rust. | High | Do not invent a Rust-only extension contract until Go hook/module semantics are already matched. |
| Expanded observability UX | Better dashboards, richer traces, friendlier admin tooling. | Medium | Useful, but operators first need compatible metrics and debugging semantics. |
| New privacy or identity features not already in Go PBS deployments | Marketable platform evolution. | High | Defer unless a concrete cutover account depends on the feature today. |
| Adapter onboarding acceleration or codegen improvements | Faster future bidder maintenance. | Medium | Valuable once parity is complete; not a substitute for missing adapter behavior today. |
| Rust-only cleanup of awkward Go semantics | Cleaner internal design. | High | A legitimate long-term goal, but not before the Rust server can mimic the current external contract. |

## Anti-Features

Things that would undermine parity or make production cutover less trustworthy.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| "Subset parity" release that omits endpoints, media types, adapters, modules, or privacy behaviors | Forces deployment-specific exceptions and proves the Rust server is not a drop-in replacement. | Keep the release bar at full documented production surface for Go-compatible cutover. |
| Config rewrites disguised as modernization | Requires operators to translate proven Go configs into a new mental model, which defeats replacement value. | Preserve config keys, defaults, precedence, and startup expectations; refactor internally only. |
| Rust-only semantics for warnings, debug output, or response extensions | Breaks troubleshooting and golden-response comparisons even when auction logic is close. | Match Go response shape and error/debug behavior before improving internals. |
| Reordering or simplifying hook/module execution semantics | Modules are behavior-changing extensions; ordering and timeout differences can change auctions materially. | Preserve Go execution-plan semantics and stage behavior exactly. |
| Treating unsupported bidders as acceptable if "major bidders work" | Publishers and hosts rely on the full adapter estate, not a popularity shortlist. | Keep bidder parity as an explicit release gate, including aliases, params, and sync metadata. |
| Deferring cache parity while claiming auction parity | Video, SDK, stored response, and event workflows depend on cache behavior. | Treat cache support as a first-class parity bucket, not a follow-up enhancement. |
| Chasing new product features before regression equivalence | Burns time while leaving unknown behavior gaps in core monetization paths. | Use all available capacity on parity gaps and parity tests first. |
| Replacing behavioral verification with unit-only confidence | Hot-path parity failures usually appear in integration behavior, not isolated helpers. | Build Go-vs-Rust fixture comparison and endpoint-level regression evidence. |

## Major Feature Buckets By Parity Surface

| Bucket | Category | Complexity | Why It Matters For Cutover |
|--------|----------|------------|-----------------------------|
| Auction flow and response shaping | Table stakes | High | This is the primary business contract: same request in, same monetization-relevant response out. |
| Bidders, adapters, aliases, params, syncers | Table stakes | High | Bidder surface is part of the product, not an implementation detail. |
| Modules and hooks | Table stakes | High | Host- and account-specific behavior often depends on them. |
| Cache, stored requests, stored responses, events | Table stakes | High | Critical for video, SDK, debugging, and event-driven flows. |
| Config, metadata, ops endpoints, observability | Table stakes | Medium | Ops compatibility is required for safe rollout and rollback. |
| Parity proof via tests and fixtures | Table stakes | High | Without it, cutover confidence is subjective. |
| Performance improvements, richer tooling, new extension models | Differentiators | Medium/High | Valuable after parity, but not a parity gate. |
| Surface reduction, config churn, Rust-only semantics | Anti-features | High risk | These directly lower operator trust and make migration harder. |

## Feature Dependencies

```text
Config/bootstrap compatibility -> Endpoint parity
Config/bootstrap compatibility -> Bidder metadata and params parity
Stored requests/accounts/default request -> Core auction lifecycle parity
Core auction lifecycle parity -> Media-type parity
Core auction lifecycle parity -> Price floors / targeting / debug parity
Bidder/adapter parity -> User sync parity
Bidder/adapter parity -> Bidder info endpoint parity
Prebid Cache parity -> Video parity
Prebid Cache parity -> Event/vtrack parity
Prebid Cache parity -> Stored response workflows
Module/hook execution parity -> Privacy enforcement parity
Module/hook execution parity -> Request/response correction behaviors
All major runtime buckets -> Test parity
```

## MVP Recommendation

For this milestone, the "MVP" is not a reduced product. It is the smallest believable cutover candidate.

Prioritize:
1. Endpoint and core auction lifecycle parity
2. Stored requests, account resolution, and cache parity
3. Full bidder/adapter/syncer/metadata parity
4. Module/hook and privacy enforcement parity
5. Config/ops compatibility
6. Go-vs-Rust parity test harness and regression suites

Defer:
- Performance tuning beyond behavioral equivalence
- New module/plugin models
- New product features or Rust-only operational improvements

## Sources

- Local project context: `.planning/PROJECT.md`
- Local architecture: `.planning/codebase/ARCHITECTURE.md`
- Local concern audit: `.planning/codebase/CONCERNS.md`
- Official Prebid Server features index: https://docs.prebid.org/prebid-server/features/pbs-feature-idx.html
- Official endpoint overview: https://docs.prebid.org/prebid-server/endpoints/pbs-endpoint-overview.html
- Official OpenRTB auction endpoint docs: https://docs.prebid.org/prebid-server/endpoints/openrtb2/pbs-endpoint-auction
- Official bidder info endpoint docs: https://docs.prebid.org/prebid-server/endpoints/info/pbs-endpoint-info.html
- Official modules docs: https://docs.prebid.org/prebid-server/pbs-modules/
- Official default request docs: https://docs.prebid.org/prebid-server/features/pbs-default-request.html
- Official privacy docs: https://docs.prebid.org/prebid-server/features/pbs-privacy.html
