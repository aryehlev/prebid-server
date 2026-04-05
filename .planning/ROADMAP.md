# Roadmap: Prebid Server Rust Parity Port

## Overview

This roadmap moves from proving Go behavior as an executable parity oracle, through the compatibility foundations that shape every request, into the public auction and support surfaces that must match production behavior, and ends with explicit cutover evidence that the Rust server can replace the Go server without changing externally visible behavior.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Parity Oracle and Coverage Matrix** - Freeze Go behavior as executable comparison evidence and parity tracking.
- [ ] **Phase 2: Config and Bootstrap Compatibility** - Match startup configuration, asset loading, and bootstrap semantics.
- [ ] **Phase 3: Stored Data and Account Resolution** - Preserve stored artifact merges and account-derived behavior.
- [ ] **Phase 4: Core Auction Lifecycle** - Match shared auction validation, timeout, and response-shaping behavior.
- [ ] **Phase 5: Public Auction Endpoint Parity** - Deliver Go-compatible `/openrtb2` endpoint behavior across core media flows.
- [ ] **Phase 6: Shared Policy, Privacy, and Identity** - Match auction policy controls, regulatory enforcement, and identity behavior.
- [ ] **Phase 7: Bidder and Adapter Breadth** - Port the full bidder surface with compatible bidder metadata behavior.
- [ ] **Phase 8: Hooks, Cache, and Support Flows** - Match hook/module execution plus cache and support endpoint behavior.
- [ ] **Phase 9: Operations Surfaces and Cutover Proof** - Prove operational compatibility and gate production replacement.

## Phase Details

### Phase 1: Parity Oracle and Coverage Matrix
**Goal**: The team can measure Rust-vs-Go behavior consistently before deeper parity work proceeds.
**Depends on**: Nothing (first phase)
**Requirements**: PARI-01, PARI-02
**Success Criteria** (what must be TRUE):
  1. The team can run automated Go-vs-Rust endpoint comparisons and receive categorized mismatch output by parity area.
  2. The team can inspect a compatibility matrix that tracks parity status across endpoints, config surfaces, bidders, modules, cache behavior, and support services.
  3. When a comparison fails, the output is specific enough to place the mismatch into the next implementation phase without manual guesswork.
**Plans**: TBD

### Phase 2: Config and Bootstrap Compatibility
**Goal**: Operators can start Rust with the same configuration inputs and bootstrap assets they use for Go.
**Depends on**: Phase 1
**Requirements**: CONF-01, CONF-02
**Success Criteria** (what must be TRUE):
  1. Operators can start the Rust server with the same config files, environment bindings, defaults, and precedence rules used by Go and observe the same effective configuration.
  2. Startup loads bidder metadata, bidder params schemas, static assets, and default request inputs from the same formats and locations used by Go.
  3. Missing or invalid config inputs produce Go-compatible startup failures or fallback behavior.
**Plans**: TBD

### Phase 3: Stored Data and Account Resolution
**Goal**: Requests that depend on stored artifacts and account state behave the same in Rust as they do in Go.
**Depends on**: Phase 2
**Requirements**: STRD-01, STRD-02
**Success Criteria** (what must be TRUE):
  1. Auction callers can rely on stored request, stored impression, and stored response references producing the same merged request and result as Go.
  2. Existing account data and defaults resolve to the same derived runtime behavior in Rust as in Go.
  3. Missing or malformed stored data and account lookups surface the same externally visible fallback or error behavior as Go.
**Plans**: TBD

### Phase 4: Core Auction Lifecycle
**Goal**: The shared auction flow matches Go before the project expands to full endpoint and bidder breadth.
**Depends on**: Phase 3
**Requirements**: AUCT-01, AUCT-02
**Success Criteria** (what must be TRUE):
  1. The same logical request inputs produce materially identical validation outcomes and warning accumulation in Rust and Go.
  2. Timeouts, cancellations, and partial results propagate through the auction lifecycle with the same externally visible behavior as Go.
  3. Final auction responses are shaped the same way as Go for successful and partially successful requests.
**Plans**: TBD

### Phase 5: Public Auction Endpoint Parity
**Goal**: Callers can use the primary `/openrtb2` auction surfaces in Rust with Go-compatible media-flow behavior.
**Depends on**: Phase 4
**Requirements**: ENDP-01, ENDP-02, ENDP-03, AUCT-04
**Success Criteria** (what must be TRUE):
  1. Auction callers can use `/openrtb2/auction` and receive Go-compatible statuses, warnings, and response extensions for equivalent requests.
  2. Auction callers can use `/openrtb2/amp` and `/openrtb2/video` with the same request normalization and response behavior they get from Go.
  3. Banner, video, native, audio, multiformat, interstitial, AMP, and long-form video flows behave externally the same as Go across these public auction endpoints.
**Plans**: TBD

### Phase 6: Shared Policy, Privacy, and Identity
**Goal**: Shared auction controls and identity-sensitive behavior match Go for the same logical inputs.
**Depends on**: Phase 5
**Requirements**: AUCT-03, IDEN-01, PRIV-01
**Success Criteria** (what must be TRUE):
  1. Price floors, currency conversion, bid adjustments, aliases, multibid, targeting, seat non-bids, request IDs, and debug output behave the same as Go for the same requests.
  2. Privacy and regulatory inputs produce the same externally visible enforcement and passthrough behavior as Go.
  3. Existing publisher and bidder integrations observe the same user sync, cookie state, cooperative sync, and EID-related behavior as Go.
**Plans**: TBD

### Phase 7: Bidder and Adapter Breadth
**Goal**: Rust supports the same bidder and adapter surface area that production Go deployments depend on.
**Depends on**: Phase 6
**Requirements**: BIDD-01, BIDD-02
**Success Criteria** (what must be TRUE):
  1. Each supported bidder and adapter issues Go-compatible outbound requests and parses responses into equivalent bid results.
  2. Bidder aliases, bidder params validation, syncer metadata, bidder capability declarations, and bidder info behavior match Go across the supported bidder surface.
  3. Operators can treat Rust as supporting the same bidder roster as Go rather than a reduced compatibility subset.
**Plans**: TBD

### Phase 8: Hooks, Cache, and Support Flows
**Goal**: Runtime side effects outside the primary `/openrtb2` happy path remain behaviorally compatible with Go.
**Depends on**: Phase 7
**Requirements**: MODL-01, CACH-01, CACH-02, ENDP-04, ENDP-05
**Success Criteria** (what must be TRUE):
  1. Host-level and account-level module and hook configuration produces the same execution-plan merging, stage ordering, timeout handling, and side effects as Go.
  2. Prebid Cache writes, TTL handling, application storage, and video caching behave the same as Go.
  3. Stored response workflows and cache-linked event flows behave the same as Go.
  4. `/cookie_sync`, `/setuid`, `/getuids`, `/event`, and `/vtrack` expose the same externally visible behavior as Go.
**Plans**: TBD

### Phase 9: Operations Surfaces and Cutover Proof
**Goal**: Operators have cutover-grade evidence that the Rust server can replace the Go server in production.
**Depends on**: Phase 8
**Requirements**: ENDP-06, OPER-01, PARI-03, PARI-04
**Success Criteria** (what must be TRUE):
  1. Operators can use `/status` and `/info/*` plus existing logs, metrics, and troubleshooting workflows against Rust with behavior compatible with Go.
  2. The team can run sampled shadow comparison or equivalent traffic replay and inspect categorized Go-vs-Rust mismatches before cutover.
  3. Production replacement is gated by passing parity suites that cover core auction flows, long-tail endpoints, bidder breadth, cache behavior, hooks/modules, privacy, and config compatibility.
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 -> 9

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Parity Oracle and Coverage Matrix | 0/TBD | Not started | - |
| 2. Config and Bootstrap Compatibility | 0/TBD | Not started | - |
| 3. Stored Data and Account Resolution | 0/TBD | Not started | - |
| 4. Core Auction Lifecycle | 0/TBD | Not started | - |
| 5. Public Auction Endpoint Parity | 0/TBD | Not started | - |
| 6. Shared Policy, Privacy, and Identity | 0/TBD | Not started | - |
| 7. Bidder and Adapter Breadth | 0/TBD | Not started | - |
| 8. Hooks, Cache, and Support Flows | 0/TBD | Not started | - |
| 9. Operations Surfaces and Cutover Proof | 0/TBD | Not started | - |
