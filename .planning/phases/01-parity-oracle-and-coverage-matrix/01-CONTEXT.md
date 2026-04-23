# Phase 1: Parity Oracle and Coverage Matrix - Context

**Gathered:** 2026-04-05
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase establishes the executable parity oracle and the coverage matrix that measure Go-versus-Rust behavior before deeper parity implementation proceeds. It defines how parity evidence is produced, what counts as a mismatch, how coverage is tracked, and how the harness widens from the core auction flow to the rest of the product surface.

</domain>

<decisions>
## Implementation Decisions

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

### the agent's Discretion
- Exact storage format and layout for generated diff artifacts, so long as they remain easy to inspect and categorize.
- Exact implementation details of the comparison runner, so long as it honors fixtures-plus-live execution and narrow normalization.
- Exact presentation format of the coverage matrix, so long as it stays domain-level and usable during later planning and execution.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Definition
- `.planning/ROADMAP.md` — Defines Phase 1 scope, dependencies, and success criteria for the parity oracle and coverage matrix.
- `.planning/REQUIREMENTS.md` — Defines `PARI-01` and `PARI-02`, plus the broader v1 parity obligations that the oracle must eventually measure.
- `.planning/PROJECT.md` — Captures the non-negotiable project constraints: exact Go behavior, compatible config/ops behavior, and full production-replacement parity.

### Existing Go Comparison Assets
- `exchange/exchange_test.go` — Contains comparison helpers such as `diffOrtbRequests` and `diffOrtbResponses`, including order-insensitive handling that informs the narrow-normalization rule.
- `endpoints/openrtb2/auction_test.go` — Contains bid-response comparison and warning-comparison helpers that show how Go tests already normalize selected nondeterministic output.

### Existing Rust Integration Points
- `rust/Cargo.toml` — Defines the Rust workspace crates that the parity harness and matrix will need to exercise and report against.
- `rust/crates/exchange/src/tests.rs` — Current Rust exchange-side test surface; useful for identifying where fixture-driven parity assertions can connect first.

### Codebase Guidance
- `.planning/codebase/TESTING.md` — Summarizes current Go testing patterns, fixture organization, and comparison style across the repo.
- `.planning/codebase/STRUCTURE.md` — Summarizes where the Go and Rust domains live and where new parity-oracle code can connect.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `exchange/exchange_test.go`: Existing JSON-based request/response diff helpers can inform the cross-language comparison model.
- `endpoints/openrtb2/auction_test.go`: Existing endpoint-level warning and bid-response comparison logic already demonstrates acceptable order normalization.
- `rust/crates/exchange/src/tests.rs`: Existing Rust exchange tests provide an initial hook point for porting or consuming shared fixture cases.
- `rust/Cargo.toml`: The Rust workspace already separates `exchange`, `endpoints`, `server`, `config`, and `cache`, which lines up well with the planned domain-level parity matrix.

### Established Patterns
- Go-side parity-friendly tests already compare semantic JSON rather than byte-for-byte wire output in places where array ordering is not operationally meaningful.
- The Go repo keeps tests close to the code they exercise and uses fixture/sample-driven validation for important auction and endpoint behavior.
- The Rust workspace is already domain-sliced rather than monolithic, which enables matrix reporting by surface instead of by crate internals.

### Integration Points
- The main auction flow should be the first parity harness target because both Go endpoint/exchange tests and Rust exchange/endpoints crates already exist there.
- Domain-level matrix reporting can align with the roadmap’s later implementation phases without forcing Phase 1 to solve every surface immediately.
- The comparison runner should connect to both the Go test/fixture ecosystem and the Rust workspace entry surfaces so later phases can widen coverage without replacing the harness.

</code_context>

<specifics>
## Specific Ideas

- Use fixtures as the stable executable spec and live side-by-side execution as the runtime validation layer.
- Normalize only clearly nondeterministic ordering and generated values.
- Track parity at the domain/surface level from the start.
- Prove the harness on the main auction flow first, but keep it extensible to the rest of the server surface.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---
*Phase: 01-parity-oracle-and-coverage-matrix*
*Context gathered: 2026-04-05*
