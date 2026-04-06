# Phase 01: Parity Oracle and Coverage Matrix - Research

**Researched:** 2026-04-05
**Domain:** Cross-language parity harness and domain-level compatibility reporting
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### Oracle Input Strategy
- **D-01:** Phase 1 should use both shared fixtures and live Go-versus-Rust side-by-side execution.
- **D-02:** Shared fixtures are the baseline executable spec, while side-by-side execution is the validation layer that proves runtime behavior against the same logical cases.

### Comparison Strictness
- **D-03:** The oracle should use narrow normalization only.
- **D-04:** Only clearly nondeterministic ordering and generated values should be normalized away; all other differences are real mismatches that must be surfaced.

### Coverage Matrix Shape
- **D-05:** The parity matrix should start at the domain/surface level rather than phase-only or per-requirement granularity.
- **D-06:** Initial matrix domains should cover endpoints, config/bootstrap, stored data, auction lifecycle, policy/privacy/identity, bidders/adapters, hooks/modules, cache/support flows, and cutover proof.

### Initial Rollout Pattern
- **D-07:** Phase 1 should prove the oracle pattern on the main auction flow first.
- **D-08:** Even though the first working slice is the main auction flow, the harness and matrix must be designed so `/openrtb2/amp`, `/openrtb2/video`, support endpoints, cache-linked flows, and other domains can plug in without redesign.

### Claude's Discretion
- Exact storage format and layout for generated diff artifacts, so long as they remain easy to inspect and categorize.
- Exact implementation details of the comparison runner, so long as it honors fixtures-plus-live execution and narrow normalization.
- Exact presentation format of the coverage matrix, so long as it stays domain-level and usable during later planning and execution.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PARI-01 | The team can run an automated Go-vs-Rust diff harness that compares endpoint behavior and highlights mismatches by category. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md] | Reuse Go fixture corpus plus in-process Go and Rust HTTP execution seams, then emit domain-tagged mismatch records instead of plain text diffs. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] |
| PARI-02 | The project maintains a compatibility matrix that tracks parity status across endpoints, config surfaces, bidders, modules, cache behavior, and support services. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md] | Use the locked domain list from Phase 1 context and roadmap as first-class matrix rows, with evidence links back to fixture cases and diff artifacts. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |
</phase_requirements>

## Summary

Phase 1 should extend the repo's existing test seams, not invent a new testing model. The Go codebase already has a rich JSON fixture corpus for `/openrtb2/auction`, a parser for the fixture schema, and semantic comparison helpers that deliberately ignore only order-sensitive noise in warnings and seatbid ordering. Rust already exposes both a workspace-wide test surface and an in-process `axum` router seam using `tower::ServiceExt::oneshot`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [CITED: https://pkg.go.dev/net/http/httptest] [CITED: https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html]

The right Phase 1 shape is a two-layer oracle. Layer 1 reuses shared fixtures as the stable executable spec. Layer 2 runs the same logical case through Go and Rust endpoint handlers and emits categorized mismatches mapped directly to the domain buckets already locked in context: endpoints, config/bootstrap, stored data, auction lifecycle, policy/privacy/identity, bidders/adapters, hooks/modules, cache/support flows, and cutover proof. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md]

No new dependency is required to stand up the first slice. The repo already contains the Go fixture/parser/comparator pieces, Rust router tests, Rust workspace manifests, and the local machine has `go`, `cargo`, `rustc`, and `node` installed. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: local environment `go version`, `cargo --version`, `rustc --version`, `node --version`]

**Primary recommendation:** Reuse the existing Go sample-request corpus and the current Go/Rust in-process HTTP test seams to build an auction-first parity oracle with narrow normalization and domain-tagged mismatch output. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

## Standard Stack

### Core
| Library / Tool | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Go `testing` + `net/http/httptest` | Go module target `1.23.0`; local `go1.25.7` available. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] [VERIFIED: local environment `go version`] | In-process execution of Go handlers and HTTP assertions. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] | This is already the dominant Go endpoint-testing pattern in the repo and the standard library explicitly provides request/recorder/server helpers for HTTP testing. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md] [CITED: https://pkg.go.dev/net/http/httptest] |
| `github.com/stretchr/testify` | `v1.8.1`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] | Structural assertions and `JSONEq`-style semantic comparison. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] | It is already the repo-default Go assertion layer. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md] |
| `github.com/buger/jsonparser` | `v1.1.1`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] | Extract fixture fields and nested warning sections without building a second fixture parser. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] | The repo already uses it for fixture decoding and warning comparison. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] |
| Rust `cargo test` / libtest | Workspace manifest uses resolver `2`; local `cargo 1.93.0-nightly` and `rustc 1.93.0-nightly` are available. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: local environment `cargo --version`] [VERIFIED: local environment `rustc --version`] | Run targeted or workspace-wide Rust tests for parity seams. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/src/tests.rs] | Cargo already supports package-targeted and workspace-wide test execution; the workspace structure is already in place. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [CITED: https://doc.rust-lang.org/cargo/commands/cargo-test.html] [CITED: https://doc.rust-lang.org/cargo/reference/workspaces.html?highlight=workspace] |
| `axum` + `tower::ServiceExt::oneshot` | `axum 0.7`, `tower 0.4`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/Cargo.toml] | In-process execution of Rust routes without binding external ports. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] | The Rust endpoint crate already uses this exact test seam. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [CITED: https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html] |
| `serde_json` | Workspace `1.x`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] | Semantic JSON comparison and response normalization on the Rust side. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/src/tests.rs] | It is already a shared workspace dependency across core crates. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] |

### Supporting
| Library / Tool | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | Workspace `1.x`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] | Async runtime for Rust endpoint and exchange tests. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/src/tests.rs] | Required for `#[tokio::test]` cases and async router execution. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] |
| `wiremock` | `0.6.5`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/Cargo.toml] | Adapter-side outbound HTTP stubbing once the oracle expands beyond empty-adapter cases. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/Cargo.toml] | Use when parity cases need deterministic external bidder responses on the Rust side. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/Cargo.toml] |
| `./validate.sh` | repo script. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] | Go formatting, `go test`, optional race, and `go vet` gates. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] | Use for full Go verification after parity harness changes touch Go code. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Existing Go fixture schema | A new Rust-only fixture format | Reject this. The repo already has a typed parser and a large sample corpus, so a second schema would create drift immediately. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json] |
| Semantic JSON comparison with narrow normalization | Byte-for-byte response comparison | Reject this. Go already treats warning order and seatbid ordering as non-semantic in selected comparisons, so raw body diffing would create false mismatches. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] |
| In-process handler/router execution for the first slice | Child-process server orchestration from day one | Use in-process first. It is faster, deterministic, and already supported by both stacks; process-level startup parity can widen later without redesigning the oracle. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] |

**Installation:**
```bash
# No new dependencies recommended for Phase 1.
go test ./exchange ./endpoints/openrtb2
cargo test --manifest-path rust/Cargo.toml -p pbs-exchange -p pbs-endpoints
```

**Version verification:** Repo-pinned versions were verified from `go.mod` and `rust/Cargo.toml`, and the required local toolchain was verified from the local shell. No new package selection is necessary for Phase 1. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: local environment `go version`, `cargo --version`, `rustc --version`, `node --version`]

## Architecture Patterns

### Recommended Project Structure
```text
parity/
├── fixtures/        # adapters over existing Go sample-request files; do not duplicate the corpus
├── oracle/          # loaders, normalizers, executors, diff categorizer
├── matrix/          # domain rows, evidence links, status generation
└── testdata/        # minimal parity-specific expectations and approved normalization cases
```

This is the cleanest boundary for cross-language harness code because the phase spans Go fixtures, Rust execution, and planner-facing reporting rather than product runtime code paths.

### Pattern 1: Shared Fixture Oracle
**What:** Parse the existing Go JSON fixture schema once and treat it as the canonical case description for both runtimes. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]

**When to use:** For every Phase 1 comparison case, starting with `/openrtb2/auction` because that surface already has the richest shared fixture set. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Example:**
```go
// Source: endpoints/openrtb2/test_utils.go
fileData, err := os.ReadFile(caseFile)
if err != nil {
	t.Fatal(err)
}

tc, err := parseTestData(fileData, caseFile)
if err != nil {
	t.Fatal(err)
}
```

### Pattern 2: In-Process Endpoint Comparison
**What:** Execute Go handlers with `httptest` and Rust routes with `Router::oneshot`, then compare status code plus semantic JSON payloads. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [CITED: https://pkg.go.dev/net/http/httptest] [CITED: https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html]

**When to use:** For the first working slice of PARI-01, before adding broader startup/bootstrap or process-level execution. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Example:**
```rust
// Source: rust/crates/endpoints/src/lib.rs
let router = create_router(test_state());
let req = Request::builder()
    .method("POST")
    .uri("/openrtb2/auction")
    .header("content-type", "application/json")
    .body(Body::from(payload))
    .unwrap();

let resp = router.oneshot(req).await.unwrap();
let status = resp.status();
let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
```

### Pattern 3: Narrow Normalization
**What:** Normalize only the response fields that Go tests already treat as order-insensitive or generated-noise, then emit all remaining diffs as real mismatches. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**When to use:** Inside the diff engine only; not in fixture generation and not as a post-hoc triage escape hatch. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Example:**
```go
// Source: exchange/exchange_test.go
actualJSON, _ := jsonutil.Marshal(actualSeats)
expectedJSON, _ := jsonutil.Marshal(expectedSeats)
assert.JSONEq(t, string(expectedJSON), string(actualJSON), description)
```

### Pattern 4: Domain-Tagged Mismatch Records
**What:** Attach a domain label at diff-emission time so every failure is already assignable to a later roadmap surface. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**When to use:** On every mismatch artifact and on every matrix row update. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md]

**Example:**
```json
{
  "case": "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
  "endpoint": "/openrtb2/auction",
  "domain": "auction_lifecycle",
  "kind": "response_body",
  "path": "$.seatbid.appnexus.bid.appnexus-bid.ext.prebid.type",
  "go": "banner",
  "rust": "video"
}
```

### Anti-Patterns to Avoid
- **Rust-only fixture copies:** They would fork the corpus away from the canonical Go test cases. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]
- **Broad normalization:** The context explicitly forbids hiding non-deterministic-looking mismatches unless they are truly generated noise or ordering noise. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]
- **Phase-only pass/fail output:** Phase 1 success requires mismatches to be placeable into later implementation phases without manual guesswork. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md]
- **Starting with external process orchestration only:** Both stacks already have in-process seams, so beginning with ports and process management would add flake before the oracle semantics are proven. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Go HTTP execution | Raw socket/server scaffolding | `httptest.NewRequest`, `httptest.NewRecorder`, and existing handler tests. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [CITED: https://pkg.go.dev/net/http/httptest] | The repo already uses these helpers extensively, and the standard library is designed for exactly this testing mode. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md] |
| Rust HTTP execution | External test-only server startup for every case | `pbs_endpoints::create_router` plus `oneshot`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] [CITED: https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html] | The endpoint crate already proves this seam works. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs] |
| Fixture schema | New parity JSON format | Existing `sample-requests` schema parsed by `parseTestData`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] | Reusing the existing corpus keeps Go as the source of truth. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/PROJECT.md] |
| Response comparison | String diff of raw JSON | Existing semantic comparators plus Rust-side `serde_json` normalization. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] | The Go code already encodes the accepted order-insensitive semantics. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] |
| Matrix tracking | Spreadsheet or ad hoc notes | Repo-committed, machine-readable rows plus a human-readable rendered view. | The planner and later phases need stable, diffable parity evidence inside the repo. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md] |

**Key insight:** Phase 1 is an evidence system, not a product-surface rewrite, so its fastest path is to reuse the Go repo's fixture and comparator semantics as the oracle baseline. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/PROJECT.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go]

## Common Pitfalls

### Pitfall 1: Over-Normalizing Real Differences
**What goes wrong:** The harness sorts, strips, or rewrites too much, so real parity bugs disappear. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Why it happens:** Existing Go tests already contain some narrow order-insensitive logic, and it is easy to expand that logic beyond what the context allows. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go]

**How to avoid:** Restrict normalization to the same classes of nondeterminism already present in Go helpers: warning order, seatbid order, and generated values explicitly approved by the phase. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Warning signs:** Many cases suddenly "match" after adding a new normalizer, or mismatch artifacts stop naming precise JSON paths.

### Pitfall 2: Comparing Only Happy-Path Response Bodies
**What goes wrong:** The harness reports parity on successful bodies while missing status-code and error-path drift. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md]

**Why it happens:** The fixture corpus contains both success and failure cases, and Go endpoint tests explicitly assert return codes and error messages in addition to bid responses. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]

**How to avoid:** Make status code, error text, warnings, validated request, and bidder-request expectations first-class diff subjects for PARI-01. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go]

**Warning signs:** Cases with `expectedErrorMessage` or `expectedReturnCode != 200` are excluded from the parity corpus. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]

### Pitfall 3: Forking the Fixture Corpus
**What goes wrong:** Rust gets its own fixture files or transformed expected payloads, and the two suites stop measuring the same logical cases. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]

**Why it happens:** The existing Go fixture shape is detailed enough that creating a second schema can look easier than writing an adapter. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json]

**How to avoid:** Write a loader over the existing JSON schema instead of converting the corpus. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go]

**Warning signs:** A parity case exists in Rust that has no source fixture path back into `endpoints/openrtb2/sample-requests/`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go]

### Pitfall 4: Reporting Mismatches Without Roadmap Domains
**What goes wrong:** A diff exists, but no one can tell whether it belongs to config, auction lifecycle, bidders, modules, or cache. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md]

**Why it happens:** Plain-text diffs are easy to generate and hard to route.

**How to avoid:** Use the Phase 1 locked domain list as the only allowed mismatch categories and require every artifact to carry one. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]

**Warning signs:** Reviewers manually interpret each failure before it can be assigned to a later phase. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md]

## Code Examples

Verified patterns from official and repo sources:

### Go Handler Execution
```go
// Source: net/http/httptest docs + endpoints/openrtb2/auction_test.go
request := httptest.NewRequest("POST", "/openrtb2/auction", bytes.NewReader(payload))
recorder := httptest.NewRecorder()
auctionEndpointHandler(recorder, request, nil)

status := recorder.Code
body := recorder.Body.Bytes()
```

### Rust Router Execution
```rust
// Source: rust/crates/endpoints/src/lib.rs
let router = create_router(test_state());
let req = Request::builder()
    .method("POST")
    .uri("/openrtb2/auction")
    .header("content-type", "application/json")
    .body(Body::from(payload))
    .unwrap();

let resp = router.oneshot(req).await.unwrap();
let status = resp.status();
let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
```

### Semantic JSON Diff
```go
// Source: exchange/exchange_test.go
actualJSON, err := jsonutil.Marshal(actual)
if err != nil {
	t.Fatal(err)
}

expectedJSON, err := jsonutil.Marshal(expected)
if err != nil {
	t.Fatal(err)
}

assert.JSONEq(t, string(expectedJSON), string(actualJSON))
```

### Warning Comparison
```go
// Source: endpoints/openrtb2/auction_test.go
compareWarnings(t, expectedBidResponse.Ext, actualBidResponse.Ext, "warnings.general")
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single-language Go fixture tests and isolated Rust crate tests. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/exchange/src/tests.rs] | Shared-fixture, live side-by-side Go-vs-Rust parity oracle. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md] | Phase 1 decisions locked on 2026-04-05. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md] | Moves parity evidence from informal spot checks to repeatable cross-language comparisons. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |
| Raw array-order-sensitive JSON comparisons. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] | Semantic JSON comparison with narrow order normalization. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] | Already present in current Go tests. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] | Lowers false-positive diffs while still surfacing real mismatches. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md] |

**Deprecated/outdated:**
- Byte-for-byte response equality as the default oracle strategy. It conflicts with the repo's existing Go test semantics for warning and seatbid ordering. [VERIFIED: /Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go]

## Assumptions Log

All claims in this research were verified or cited during this session. No user-confirmation assumptions are currently open.

## Open Questions (RESOLVED)

1. **Should Phase 1 include only executable auction diffs, or should the matrix also ship placeholder rows for non-auction domains?**
   - What we know: The first working oracle slice must be the main auction flow, but the matrix must start at the full domain/surface level. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md]
   - Resolution: Phase 1 should ship all domain rows immediately, even though only the main auction flow has executable coverage at first.
   - Decision: Mark the non-auction domains as `not-yet-instrumented` and attach evidence links only for the auction-first slice until later phases widen execution coverage. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md]

2. **Should side-by-side execution stay in-process for Phase 1, or also expose a CLI mode?**
   - What we know: Both Go and Rust already support in-process execution seams. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs]
   - Resolution: Phase 1 should keep side-by-side execution in-process and should not require a standalone CLI mode.
   - Decision: Build the core oracle as library code used by tests first, then add a thin CLI wrapper only if later planning or execution requires artifact generation outside `go test` or `cargo test`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Go | Go fixture execution and comparator reuse. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go] | ✓ | `go1.25.7`. [VERIFIED: local environment `go version`] | — |
| Cargo | Rust workspace test execution. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] | ✓ | `1.93.0-nightly`. [VERIFIED: local environment `cargo --version`] | — |
| Rustc | Rust crate compilation for parity tests. [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] | ✓ | `1.93.0-nightly`. [VERIFIED: local environment `rustc --version`] | — |
| Node | Existing GSD helper tooling and optional report generation. [VERIFIED: `node /Users/aryehlev/.codex/get-shit-done/bin/gsd-tools.cjs init phase-op ...`] | ✓ | `v22.14.0`. [VERIFIED: local environment `node --version`] | Bash or Go if the parity runner avoids Node. |

**Missing dependencies with no fallback:**
- None. [VERIFIED: local environment `go version`, `cargo --version`, `rustc --version`, `node --version`]

**Missing dependencies with fallback:**
- None. [VERIFIED: local environment `go version`, `cargo --version`, `rustc --version`, `node --version`]

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Mixed Go `testing` plus Rust `cargo test` / libtest. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [CITED: https://doc.rust-lang.org/cargo/commands/cargo-test.html] |
| Config file | Go uses `go.mod` plus repo scripts; Rust uses `rust/Cargo.toml`; no dedicated third-party test-runner config was found. [VERIFIED: /Users/aryehlev/Documents/prebid-server/go.mod] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md] |
| Quick run command | `go test ./exchange ./endpoints/openrtb2 && cargo test --manifest-path rust/Cargo.toml -p pbs-exchange -p pbs-endpoints`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] |
| Full suite command | `./validate.sh && cargo test --manifest-path rust/Cargo.toml --workspace`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml] |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PARI-01 | Run shared-fixture plus live side-by-side `/openrtb2/auction` comparisons and emit categorized mismatches. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md] | integration | `go test ./parity/... -run TestAuctionOracle -count=1` | ❌ Wave 0 |
| PARI-02 | Generate and validate a domain-level compatibility matrix covering the locked surfaces. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md] | unit/smoke | `go test ./parity/... -run TestCoverageMatrix -count=1` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `go test ./parity/... -count=1` once the parity package exists.
- **Per wave merge:** `go test ./exchange ./endpoints/openrtb2 && cargo test --manifest-path rust/Cargo.toml -p pbs-exchange -p pbs-endpoints`. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml]
- **Phase gate:** `./validate.sh && cargo test --manifest-path rust/Cargo.toml --workspace` plus the new parity oracle tests green. [VERIFIED: /Users/aryehlev/Documents/prebid-server/validate.sh] [VERIFIED: /Users/aryehlev/Documents/prebid-server/rust/Cargo.toml]

### Wave 0 Gaps
- [ ] `parity/oracle/auction_test.go` — cross-language auction diff harness for PARI-01.
- [ ] `parity/oracle/categorize_test.go` — mismatch taxonomy and routing assertions for PARI-01.
- [ ] `parity/matrix/matrix_test.go` — matrix completeness and evidence-link validation for PARI-02.
- [ ] `parity/testdata/` — approved parity-specific normalization and categorization samples.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Phase 1 is internal harness and reporting work, not an auth surface. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |
| V3 Session Management | no | Phase 1 does not introduce a new user session model. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |
| V4 Access Control | no | Phase 1 scope is testing and matrix reporting. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |
| V5 Input Validation | yes | Reuse the existing fixture parser, validate file roots, and treat case metadata as structured input rather than ad hoc strings. [VERIFIED: /Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go] |
| V6 Cryptography | no | No new crypto primitive is required for the Phase 1 oracle itself. [VERIFIED: /Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md] |

### Known Threat Patterns for This Phase

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Treat fixture paths as trusted input. | Tampering | Restrict the corpus to approved directories and reject path traversal or arbitrary file reads. |
| Persist raw diff artifacts that may later contain IDs, cookies, or request metadata. | Information Disclosure | Keep raw artifacts local, and redact generated identifiers or cookies before committing summary evidence. |
| Shell-concatenate Go and Rust runner arguments. | Tampering | Invoke subprocesses with explicit argument arrays if a CLI wrapper is added. |

## Sources

### Primary (HIGH confidence)
- `/Users/aryehlev/Documents/prebid-server/.planning/phases/01-parity-oracle-and-coverage-matrix/01-CONTEXT.md` - locked Phase 1 decisions and domain model.
- `/Users/aryehlev/Documents/prebid-server/.planning/REQUIREMENTS.md` - PARI-01 and PARI-02 definitions.
- `/Users/aryehlev/Documents/prebid-server/.planning/ROADMAP.md` - Phase 1 success criteria and downstream domains.
- `/Users/aryehlev/Documents/prebid-server/go.mod` - Go version and existing dependency versions.
- `/Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/auction_test.go` - current Go endpoint fixture execution and response comparison patterns.
- `/Users/aryehlev/Documents/prebid-server/endpoints/openrtb2/test_utils.go` - fixture schema and parser.
- `/Users/aryehlev/Documents/prebid-server/exchange/exchange_test.go` - existing narrow normalization helpers for requests and responses.
- `/Users/aryehlev/Documents/prebid-server/rust/Cargo.toml` - Rust workspace members and shared dependencies.
- `/Users/aryehlev/Documents/prebid-server/rust/crates/endpoints/src/lib.rs` - current Rust router and in-process route tests.
- `/Users/aryehlev/Documents/prebid-server/rust/crates/exchange/src/tests.rs` - current Rust exchange-side test coverage shape.
- `/Users/aryehlev/Documents/prebid-server/validate.sh` - Go validation command shape.
- `/Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md` - repo-wide test conventions.
- `/Users/aryehlev/Documents/prebid-server/.planning/codebase/STRUCTURE.md` - codebase boundaries for placement decisions.
- https://pkg.go.dev/net/http/httptest - official Go HTTP testing utilities.
- https://docs.rs/tower/latest/tower/util/trait.ServiceExt.html - official `oneshot` testing seam for tower services.
- https://doc.rust-lang.org/cargo/commands/cargo-test.html - official Cargo test behavior.
- https://doc.rust-lang.org/cargo/reference/workspaces.html?highlight=workspace - official Cargo workspace behavior.

### Secondary (MEDIUM confidence)
- None.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all recommended tools are already in the repo or official standard tooling.
- Architecture: HIGH - the repo already exposes the exact Go and Rust seams Phase 1 needs.
- Pitfalls: MEDIUM - most are strongly grounded in locked decisions and current helpers; a few warning-sign descriptions are inferred from likely review failure modes.

**Research date:** 2026-04-05
**Valid until:** 2026-05-05
