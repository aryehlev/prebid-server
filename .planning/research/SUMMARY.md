# Project Research Summary

**Project:** Rust Prebid Server parity port
**Domain:** Production-replacement OpenRTB bidding server parity with Go Prebid Server
**Researched:** 2026-04-05
**Confidence:** HIGH

## Executive Summary

This project is not a greenfield Rust ad server. It is a production-replacement port of Go Prebid Server, so the real product is behavioral compatibility: same config semantics, same endpoint surface, same stored-request and cache behavior, same hook/module ordering, and materially identical auction outputs under the same inputs. Experts build this kind of system by freezing incumbent behavior first, then porting around stable seams rather than redesigning the domain model.

The recommended approach is parity-first and seam-preserving. Keep the current Tokio stack and modernize it only where it supports parity work: `tokio` + `axum` + `tower`, shared `reqwest` clients with `rustls`, `serde` plus a custom compatibility loader, direct `prometheus` instrumentation, repo-owned OpenRTB/ext models, and first-class `stored-data`, `hooks`, and `modules` layers. Preserve the Go macro-architecture: thin endpoints, a deterministic exchange kernel, narrow adapters, startup-loaded static assets, and explicit Prebid Cache integration over HTTP.

The main risks are not ordinary Rust implementation risks. They are compatibility risks: stricter parsing than Go, typed decoding too early, wrong timeout propagation, hook/module execution drift, config precedence drift, and rollout without a Go-vs-Rust diff harness. Planning should treat the parity oracle, config/merge semantics, deadline model, and execution-plan fidelity as the first gates. Performance tuning, Rust-native cleanup, and new extension models should stay out of the critical path until cutover-grade parity is proven.

## Key Findings

### Recommended Stack

The stack choice is mostly settled. The remaining value is in choosing the parts that preserve behavior rather than introducing fresh semantics. The port should stay on the standard Tokio HTTP stack, but reject generic config frameworks, ORM-heavy persistence, dynamic plugin systems, and alternative cache architectures that would change the external contract.

**Core technologies:**
- `tokio` + `axum` + `tower` + `tower-http`: runtime, routing, and middleware surface without fighting the current Rust workspace direction.
- `reqwest` + `rustls`: shared outbound HTTP client path for bidders, cache, currency, and background fetches with pooled connections and predictable TLS defaults.
- `serde` / `serde_json` / `serde_yaml`: core serialization layer, but constrained by golden fixtures and compatibility rules rather than Rust-native strictness.
- Custom config compatibility loader: required to match Go PBS precedence, `PBS_` env binding, defaults, and file merge semantics; generic config crates are the wrong source of truth.
- Repo-owned `openrtb` and `openrtb-ext` crates: keep PBS-specific quirks, deprecated paths, and ext payload shape under local control.
- `jsonschema`: startup-time validation for bidder params and static schemas using precompiled validators.
- `moka`: bounded TTL caches for in-process state such as stored artifacts and schemas.
- `sqlx`: narrow database access only where Go-compatible stored request/account backends require MySQL or Postgres.
- `prometheus` + `tracing`: preserve Prometheus scraping and operator-visible diagnostics; keep OTEL optional rather than primary.
- `wiremock`, `cargo-nextest`, and targeted parity fixtures: essential for endpoint- and adapter-level regression proof.

**Critical version shifts to standardize early:**
- Rust `1.94.1` stable, edition `2024`
- `axum 0.8.x`, `tower 0.5.x`, `tower-http 0.6.x`
- `reqwest 0.13.x`, `prometheus 0.14.x`

### Expected Features

For this milestone, "MVP" means "smallest believable cutover candidate," not a reduced product. Anything operators already depend on in Go PBS is table stakes for the Rust port.

**Must have (table stakes):**
- Public endpoint parity for `/openrtb2/auction`, `/openrtb2/amp`, `/openrtb2/video`, `/cookie_sync`, `/setuid`, `/getuids`, `/status`, `/info/*`, `/event`, and `/vtrack`.
- Core auction lifecycle parity including stored request merge behavior, account resolution, validation, privacy enforcement, targeting, floors, currency, bid adjustments, aliases, multibid, debug output, and response shaping.
- Media-type parity across banner, video, native, audio, multiformat, AMP, interstitial, and long-form video flows.
- Stored request, stored response, and account parity with existing backend and merge semantics.
- Prebid Cache parity for cache writes, VAST/application storage, TTL behavior, video, and event-driven workflows.
- Full bidder, adapter, alias, params, syncer, and bidder-info parity as an explicit release gate.
- User sync and identity parity for `/cookie_sync`, `/setuid`, `/getuids`, cooperative sync, and EID-related behaviors.
- Privacy and regulatory parity for GDPR, USP/CCPA, GPP, COPPA, GPC, DSA passthrough, device LMT, and activity controls.
- Hook and module execution-plan parity, including host/account plan merging, per-stage ordering, timeouts, and side effects.
- Config/bootstrap and ops compatibility, including startup-loaded assets, defaults, health/info/admin surfaces, metrics, and operator workflows.
- Go-vs-Rust behavioral proof through parity fixtures and endpoint-level regression evidence.

**Should have (cutover-enabling, but after the core kernel is stable):**
- Request-level Go-vs-Rust diff telemetry and mismatch categorization for shadow rollout.
- Stage-level snapshots for request merge, bidder fan-out, bidder responses, and final response assembly.
- Compatibility matrix tracking parity status by config fields, endpoints, hook stages, support services, and adapters.

**Defer (post-parity):**
- Rust-native performance tuning beyond proven equivalence.
- New module/plugin systems or WASM extension models.
- Expanded observability UX beyond compatible metrics and logs.
- New privacy or identity product features not already required by Go PBS deployments.
- Adapter codegen or Rust-only internal cleanup that changes semantics or operator expectations.

### Architecture Approach

The architecture should mirror the Go server's stable seams, not reinterpret them. Keep `server` as the composition root, `config` as the compatibility layer, `endpoints` as thin HTTP-specific normalization and response code, `exchange` as the deterministic auction kernel, `adapters` as narrow protocol translators, `cache` as the Prebid Cache client, and extract `stored-data`, `hooks`, and `modules` into first-class domains. The key rule is to keep raw JSON alive through the same merge/mutation stages as Go, run the full hook lifecycle including `exitpoint`, and unify shared services before scaling adapter work.

**Major components:**
1. `config` / bootstrap — load host config, account defaults, bidder metadata, bidder params, aliases, hook plans, and static assets with Go-compatible precedence and env mapping.
2. `stored-data` — resolve stored requests, stored imps, stored responses, accounts, and related cache invalidation behavior used during normalization.
3. `hooks` + `modules` — merge host/account execution plans, execute all eight hook stages, and keep module config/runtime separation stable.
4. `exchange` — own request splitting, deadline math, bidder fan-out, privacy/policy application, cache decisions, and final OpenRTB response assembly.
5. `adapters` — translate normalized bidder requests into outbound calls and parse bidder responses without owning auction policy.
6. `endpoints` — keep `/auction`, `/amp`, `/video`, and operational routes as thin transport layers over the shared kernel.
7. `cache` / support services — keep one contract each for Prebid Cache, currency, analytics, metrics, and similar shared behavior.

### Critical Pitfalls

1. **Building before a parity oracle exists** — make the Go-vs-Rust diff harness the first deliverable so parity decisions are evidence-based rather than stylistic.
2. **Typing JSON too early** — preserve raw JSON through stored-request merges, hook mutations, and compatibility-sensitive transformations before committing to typed validation.
3. **Getting timeout and cancellation semantics wrong** — define one canonical deadline model and propagate it consistently through hooks, bidder calls, cache calls, and partial-result collection.
4. **Underestimating hook/module execution-plan fidelity** — port execution-plan representation, stage ordering, account overrides, and timeout behavior before treating module work as complete.
5. **Breaking config and cold-start semantics while "cleaning up" startup** — preserve Viper-like precedence, asset directory contracts, warm/cold cache behavior, and background refresh semantics before optimizing.

## Implications for Roadmap

Based on the combined research, the roadmap should be organized around parity dependencies, not around user-facing feature popularity or adapter count. The right order is to freeze the oracle, stabilize compatibility boundaries, then widen runtime parity on top of those seams.

### Phase 1: Parity Oracle and Diff Harness
**Rationale:** The port cannot be managed responsibly until Go behavior is frozen as an executable spec.
**Delivers:** Cross-language fixture harness, endpoint/status/header/body diffs, canonical mismatch taxonomy, and initial stage-level snapshots.
**Addresses:** Test parity, public endpoint contract parity, rollout readiness.
**Avoids:** Porting source structure instead of behavior; silent wire drift.

### Phase 2: Config, Bootstrap, and Merge Semantics
**Rationale:** Exact config precedence, asset loading, stored-request merge behavior, and request parsing semantics are prerequisites for every downstream parity claim.
**Delivers:** Custom compatibility loader, startup precedence matrix, raw-JSON merge pipeline, bidder metadata/params loading, account and stored-data resolution seams.
**Uses:** `serde`, `serde_json`, `serde_yaml`, `jsonschema`, `moka`, narrow `sqlx` backends where required.
**Implements:** `config` and `stored-data` foundations.
**Avoids:** Early typed decoding, stricter-than-Go validation, config drift.

### Phase 3: Auction Core, Deadlines, and Shared Policy
**Rationale:** Adapter parity is meaningless until the exchange kernel matches Go's shared auction behavior.
**Delivers:** Deterministic exchange flow, canonical deadline propagation, bidder fan-out, privacy/floors/currency/targeting/cache-decision parity, seat-non-bid and debug response parity.
**Uses:** `tokio`, `axum`, `tower`, shared `reqwest` clients, `CancellationToken`, repo-owned OpenRTB models.
**Implements:** `exchange` as the stable auction kernel.
**Avoids:** Wrong timeout math, policy drift hidden by passing adapter tests.

### Phase 4: Hooks, Modules, and Adapter Integration
**Rationale:** The hook lifecycle and execution-plan model materially change auction behavior, and adapter work is safer only after those seams are fixed.
**Delivers:** Full eight-stage hook lifecycle including `exitpoint`, host/account execution-plan merging, built-in module registration, and adapter ports against the stable kernel.
**Addresses:** Module/hook parity, bidder/alias/params/sync metadata parity.
**Implements:** `hooks`, `modules`, and narrow `adapters` boundaries.
**Avoids:** Flattened middleware-style hooks, account-plan incompatibility, brittle adapter ports.

### Phase 5: Cache, Warm/Cold Runtime, and Long-Tail Endpoint Parity
**Rationale:** Video, events, stored responses, and operational trust all depend on matching Prebid Cache behavior and non-happy-path runtime semantics.
**Delivers:** Unified Prebid Cache client, cold/warm/stale cache behavior, refresh failure semantics, `/event`, `/vtrack`, `/cookie_sync`, `/setuid`, `/getuids`, `/status`, and `/info/*` parity.
**Uses:** shared `reqwest` cache/service clients, `moka`, direct `prometheus` instrumentation, `tracing`.
**Implements:** cache client and operational endpoint surface.
**Avoids:** warm-only correctness and endpoint drift outside `/openrtb2/auction`.

### Phase 6: Shadow Rollout and Cutover Proof
**Rationale:** Production replacement requires comparison-grade evidence under real traffic, not only fixture confidence.
**Delivers:** Sampled shadow traffic diffing, operator-comparable dashboards/metrics, cutover gates, and explicit accepted-divergence list if any remain.
**Addresses:** Observability/ops compatibility and rollout safety.
**Avoids:** Shipping without localization-grade telemetry or operator trust.

### Phase Ordering Rationale

- Phase 1 comes first because every later argument depends on a frozen oracle and mismatch taxonomy.
- Phase 2 precedes Phase 3 because exchange behavior cannot be trusted until config, stored artifacts, and request normalization are faithful.
- Phase 3 precedes large-scale adapter work because most monetization-visible drift lives in shared policy, not per-bidder HTTP translation.
- Phase 4 follows the stable kernel because hooks/modules and adapters should plug into fixed seams rather than force later rewrites.
- Phase 5 groups cache parity, cold-start semantics, and long-tail endpoints because they share the same support-service and operational compatibility surfaces.
- Phase 6 is last because shadow rollout is a proving step, not a substitute for architectural parity work.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2:** Go config precedence, raw JSON merge semantics, and accepted malformed-input behavior need explicit parity matrices and fixture capture.
- **Phase 4:** Hook/module execution-plan merging, stage semantics, and account override behavior are high-risk and easy to under-specify.
- **Phase 5:** Warm/cold cache behavior, refresh failures, and multi-backend stored-data semantics need outage/restart scenario planning, not just happy-path implementation.

Phases with standard patterns (skip research-phase):
- **Phase 1:** Diff harness and parity corpus creation are straightforward once the comparison boundaries are chosen.
- **Phase 3:** The Rust stack and exchange-kernel implementation patterns are well understood; the hard part is matching Go behavior, not picking technology.
- **Phase 6:** Shadow rollout mechanics and observability plumbing are standard once mismatch categories and operator metrics are defined.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Recommendations are backed by current official Rust crate/docs and fit the existing Rust workspace direction. |
| Features | HIGH | Table-stakes parity surface is strongly grounded in official Prebid Server endpoint, feature, privacy, and module documentation. |
| Architecture | MEDIUM | The macro-shape is clear and well supported by Go/Rust code inspection, but exact seam extraction in the current workspace still needs implementation-level validation. |
| Pitfalls | HIGH | Risks are concrete, repeatedly implied by the domain, and map directly to known Go/Rust compatibility failure modes. |

**Overall confidence:** HIGH

### Gaps to Address

- Accepted Go quirks vs intended behavior: planning should explicitly classify where Rust must match incumbent bugs and where divergence can be staged behind flags or post-cutover work.
- Exact long-tail bidder and module parity status: roadmap planning needs a live compatibility matrix rather than assuming feature-complete coverage from broad category labels.
- Real operator config corpus: planning should collect representative deployment trees, env overrides, and startup asset layouts to validate the compatibility loader.
- Shadow traffic normalization rules: the rollout plan needs a precise list of nondeterministic fields that may be normalized in diffs and a list that must remain exact.

## Sources

### Primary (HIGH confidence)
- Official Prebid Server feature index and endpoint documentation — parity surface, routes, privacy, default request, modules, and operational expectations.
- Official Prebid Server module docs — hook stages, execution-plan semantics, and module architecture expectations.
- Rust crate documentation for `tokio`, `axum`, `tower`, `tower-http`, `reqwest`, `rustls`, `serde`, `tracing`, `prometheus`, `jsonschema`, `moka`, `sqlx`, `wiremock`, `proptest`, and `criterion` — current stack/tooling guidance.
- Rust stable release notes and edition docs — toolchain/version recommendations.
- Local repository sources in Go and Rust — existing seams, current divergences, and realistic port constraints.

### Secondary (MEDIUM confidence)
- Prebid Cache API/docs and local architecture/testing artifacts — cache/runtime expectations and parity-proof strategy.
- Viper behavior and Go/Rust serialization references — config precedence and permissive parsing constraints.

### Tertiary (LOW confidence)
- None identified in the research set.

---
*Research completed: 2026-04-05*
*Ready for roadmap: yes*
