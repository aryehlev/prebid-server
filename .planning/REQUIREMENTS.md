# Requirements: Prebid Server Rust Parity Port

**Defined:** 2026-04-05
**Core Value:** The Rust server can replace the Go server for production traffic without changing application behavior.

## v1 Requirements

Requirements for the production-replacement parity release. Each requirement maps to exactly one roadmap phase.

### Endpoint Surface

- [ ] **ENDP-01**: Auction callers can send a standard OpenRTB auction request to the Rust `/openrtb2/auction` endpoint and receive response semantics that match the Go server, including status codes, warnings, and response extensions.
- [ ] **ENDP-02**: Auction callers can use the Rust `/openrtb2/amp` endpoint with the same request normalization and response behavior as the Go server.
- [ ] **ENDP-03**: Auction callers can use the Rust `/openrtb2/video` endpoint with the same simplified-video request handling and response behavior as the Go server.
- [ ] **ENDP-04**: Publishers and sync partners can use the Rust `/cookie_sync`, `/setuid`, and `/getuids` endpoints with the same externally visible behavior as the Go server.
- [ ] **ENDP-05**: Integrations that depend on the Rust `/event` and `/vtrack` endpoints observe the same request and response behavior as in the Go server.
- [ ] **ENDP-06**: Operators and tooling can use the Rust `/status` and `/info/*` endpoints with behavior compatible with the Go server.

### Configuration and Bootstrap

- [ ] **CONF-01**: Operators can start the Rust server with the same config files, environment variable bindings, defaults, and precedence rules used by the Go server.
- [ ] **CONF-02**: The Rust server loads bidder metadata, bidder params schemas, static assets, and default request inputs from the same formats and locations expected by the Go server.

### Stored Data and Account Resolution

- [ ] **STRD-01**: Auction callers can rely on the Rust server to apply the same stored request, stored impression, and stored response merge semantics as the Go server.
- [ ] **STRD-02**: Operators can use existing account data and defaults with the Rust server and get the same account resolution and derived behavior as in the Go server.

### Auction Core

- [ ] **AUCT-01**: Auction callers receive materially identical validation outcomes, warning accumulation, and final response shaping from the Rust auction flow for the same logical request inputs.
- [ ] **AUCT-02**: The Rust auction flow propagates timeouts, deadlines, cancellations, and partial-result behavior the same way the Go server does across hooks, bidder calls, cache calls, and response assembly.
- [ ] **AUCT-03**: The Rust server applies the same hot-path auction policy behavior as the Go server for price floors, currency conversion, bid adjustments, aliases, multibid, targeting, seat non-bid handling, request IDs, and debug output.
- [ ] **AUCT-04**: Auction callers can use banner, video, native, audio, multiformat, interstitial, AMP, and long-form video flows in Rust with the same externally visible behavior as in Go.

### Bidders and Adapters

- [ ] **BIDD-01**: Every bidder and adapter supported by the Go server is ported to Rust with matching request-building, outbound call, and response-parsing behavior.
- [ ] **BIDD-02**: Bidder aliases, bidder params validation, syncer metadata, bidder capability declarations, and bidder info behavior match the Go server across the full supported bidder surface.

### Identity and Sync

- [ ] **IDEN-01**: User sync, cookie state, cooperative sync behavior, and EID-related request handling in Rust match the Go server well enough for existing publisher and bidder integrations to behave the same way.

### Modules and Hooks

- [ ] **MODL-01**: Host-level and account-level module and hook configuration in Rust produces the same execution-plan merging, stage ordering, timeout behavior, and side effects as the Go server.

### Privacy and Regulation

- [ ] **PRIV-01**: GDPR, USP/CCPA, GPP, COPPA, GPC, DSA passthrough, device LMT, activity controls, and related privacy enforcement in Rust match the Go server's externally visible behavior.

### Cache and Support Services

- [ ] **CACH-01**: The Rust server performs Prebid Cache writes, TTL handling, application storage, video caching, and cache-related request/response behavior the same way the Go server does.
- [ ] **CACH-02**: Stored response workflows and cache-linked event flows behave the same in Rust as in the Go server.

### Operations Compatibility

- [ ] **OPER-01**: Operators can run, inspect, and troubleshoot the Rust server using metrics, logs, health/admin surfaces, and operational workflows that remain compatible with the current Go server.

### Parity Verification and Cutover Proof

- [ ] **PARI-01**: The team can run an automated Go-vs-Rust diff harness that compares endpoint behavior and highlights mismatches by category.
- [ ] **PARI-02**: The project maintains a compatibility matrix that tracks parity status across endpoints, config surfaces, bidders, modules, cache behavior, and support services.
- [ ] **PARI-03**: The Rust server supports sampled shadow comparison or equivalent traffic replay so the team can compare Go and Rust behavior before production cutover.
- [ ] **PARI-04**: Production replacement is gated by passing parity suites that cover core auction flows, long-tail endpoints, bidder breadth, cache behavior, hooks/modules, privacy, and config compatibility.

## v2 Requirements

Deferred until after feature and behavior parity.

### Performance and Reliability

- **PERF-01**: Operators can prove the Rust server meets or beats the Go server for target latency and throughput under representative load.
- **PERF-02**: Operators can prove the Rust server meets rollout-grade reliability through soak testing, failure-mode validation, and operational runbooks.

### Cleanup and Evolution

- **CLNP-01**: Deployment and tooling workflows are cleaned up and simplified after parity without breaking compatible operations.
- **CLNP-02**: Documentation is refreshed for the Rust-first implementation after parity is stable.
- **EVOL-01**: New product features beyond current Go Prebid Server behavior are added on top of the parity baseline.
- **EVOL-02**: Rust-only refactors and internal cleanup that would materially change semantics are revisited after cutover confidence is established.

## Out of Scope

Explicit exclusions for the parity release.

| Feature | Reason |
|---------|--------|
| New product features | Parity work takes priority over expanding scope |
| Behavior-changing refactors | The Rust server must match Go behavior even when the Go design is awkward |
| Deployment and tooling cleanup | Useful later, but not required to finish app-code and business-feature parity |
| Documentation cleanup | Does not directly reduce parity risk |
| Performance optimization beyond proven equivalence | Deferred until the parity surface is complete and verified |
| Reliability hardening beyond parity validation | Deferred until after the Rust server matches the Go business surface |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| ENDP-01 | Phase 5 | Pending |
| ENDP-02 | Phase 5 | Pending |
| ENDP-03 | Phase 5 | Pending |
| ENDP-04 | Phase 8 | Pending |
| ENDP-05 | Phase 8 | Pending |
| ENDP-06 | Phase 9 | Pending |
| CONF-01 | Phase 2 | Pending |
| CONF-02 | Phase 2 | Pending |
| STRD-01 | Phase 3 | Pending |
| STRD-02 | Phase 3 | Pending |
| AUCT-01 | Phase 4 | Pending |
| AUCT-02 | Phase 4 | Pending |
| AUCT-03 | Phase 6 | Pending |
| AUCT-04 | Phase 5 | Pending |
| BIDD-01 | Phase 7 | Pending |
| BIDD-02 | Phase 7 | Pending |
| IDEN-01 | Phase 6 | Pending |
| MODL-01 | Phase 8 | Pending |
| PRIV-01 | Phase 6 | Pending |
| CACH-01 | Phase 8 | Pending |
| CACH-02 | Phase 8 | Pending |
| OPER-01 | Phase 9 | Pending |
| PARI-01 | Phase 1 | Pending |
| PARI-02 | Phase 1 | Pending |
| PARI-03 | Phase 9 | Pending |
| PARI-04 | Phase 9 | Pending |

**Coverage:**
- v1 requirements: 26 total
- Mapped to phases: 26
- Unmapped: 0

---
*Requirements defined: 2026-04-05*
*Last updated: 2026-04-05 after roadmap creation*
