# Domain Pitfalls

**Domain:** Rust parity port of a mature Go Prebid Server
**Researched:** 2026-04-05
**Overall confidence:** HIGH

## Recommended Phase Map

Use these phase names when mapping pitfalls into the roadmap:

- **Phase 1: Parity Oracle and Diff Harness** — freeze incumbent Go behavior before porting deeper.
- **Phase 2: Config, Request Parsing, and Merge Semantics** — preserve config precedence, stored-request layering, and JSON compatibility.
- **Phase 3: Auction Core, Deadlines, and Policy Surfaces** — reproduce bidder fan-out, timeout math, response shaping, and shared auction logic.
- **Phase 4: Hooks, Modules, and Adapter Integration** — preserve execution-plan order, per-stage timeouts, and adapter-to-core integration behavior.
- **Phase 5: Cache, Background Refresh, and Cold-Start Semantics** — match warm/cold behavior, refresh loops, and fallback paths.
- **Phase 6: Wire Compatibility and Shadow Rollout** — prove HTTP/output/operator parity under real traffic.

## Critical Pitfalls

### Pitfall 1: Porting Source Structure Instead of Freezing Executable Behavior
**What goes wrong:** Teams start translating Go packages into Rust before they have a request-level oracle for what "same behavior" means. Mature bidding servers accumulate tolerated quirks, response-shaping edge cases, and operator-visible oddities that are not obvious from reading code.
**Why it happens:** Engineers treat the Go code as architecture to rewrite instead of the behavioral spec to emulate.
**Consequences:** "Idiomatic Rust" decisions silently change output, error handling, or rollout expectations, and parity debates get resolved by taste instead of evidence.
**Warning signs:**
- No Go-vs-Rust golden corpus for requests, responses, headers, warnings, and status codes.
- Port tasks are phrased as "implement feature X" rather than "match Go output for corpus Y".
- Engineers propose cleaning up awkward Go behavior before it has been captured in tests.
**Prevention strategy:**
- Build a parity harness first: same input/config into Go and Rust, then diff HTTP status, headers, body, debug ext, warnings, seat-non-bid output, and timeout classifications.
- Seed the corpus from existing Go fixtures, production-like captured traffic, and known bug/edge cases from `.planning/codebase/CONCERNS.md`.
- Treat every accepted mismatch as a conscious roadmap item, not an incidental cleanup.
**Which phase should address it:** **Phase 1: Parity Oracle and Diff Harness**

### Pitfall 2: Normalizing JSON Into Rust Types Too Early
**What goes wrong:** The Rust port deserializes requests, stored requests, account config, or bidder ext into typed structs before the Go system has finished all raw JSON merges and mutations. That changes semantics for missing vs `null`, duplicate keys, case-insensitive matches, unknown fields, and partial merge behavior.
**Why it happens:** Rust code feels safer when everything is typed immediately, but Prebid Server behavior depends on raw JSON handling in multiple hot paths.
**Consequences:** Stored-request overlays, hook mutations, bidder params, and request wrappers stop matching Go even when the final typed model looks reasonable.
**Warning signs:**
- Top-level auction flow starts with `serde` structs instead of a raw-JSON phase.
- `deny_unknown_fields`, eager enums, or blanket `Option<T>` use appear on OpenRTB and config boundary types.
- Shadow mismatches cluster around stored requests, account overrides, bidder params, or hook-modified payloads.
**Prevention strategy:**
- Keep raw JSON as the source of truth through every merge/mutation stage that exists in Go.
- Use typed decoding only after the Go pipeline would have committed to typed validation.
- Add focused parity fixtures for `null`, omitted fields, duplicate keys, mixed-case keys, and merge-patch removals.
**Which phase should address it:** **Phase 2: Config, Request Parsing, and Merge Semantics**

### Pitfall 3: Making Validation Stricter Than the Go Server
**What goes wrong:** The Rust implementation rejects traffic or config that the Go server currently tolerates. Go's `encoding/json` is permissive by default in ways that strongly typed Rust models often are not.
**Why it happens:** Teams optimize for correctness from first principles instead of compatibility with production behavior.
**Consequences:** More 400s, more startup failures, more bidder rejections, and rollout regressions that only appear on real traffic.
**Warning signs:**
- Rising reject rates in shadow traffic even when happy-path tests pass.
- Config files that boot in Go but fail in Rust.
- Rust deserializers use strict enums, strict unknown-field rejection, or custom validators at the parse boundary.
**Prevention strategy:**
- Split compatibility parsing from semantic validation.
- Mirror Go's accepted/rejected boundary before tightening anything.
- Build a negative corpus with malformed, partially malformed, and "weird but accepted" requests/configs from Go tests and production incidents.
**Which phase should address it:** **Phase 2: Config, Request Parsing, and Merge Semantics**

### Pitfall 4: Reproducing the Wrong Timeout and Cancellation Semantics
**What goes wrong:** The port preserves nominal `tmax` support but not the actual runtime behavior of deadline propagation, cancellation, bidder timeout deduction, and partial-result collection.
**Why it happens:** Go goroutines plus `context.Context` do not map 1:1 onto Tokio tasks, dropped futures, or HTTP client defaults.
**Consequences:** Different bidders time out, response-time metrics drift, seat-non-bid reasons change, and under load the Rust server returns materially different auctions.
**Warning signs:**
- Same input and config produce different bidder participation under latency injection.
- Rust metrics show shifted bidder timeout/error mixes compared with Go.
- Timeout logic is implemented independently in adapters, hooks, and HTTP client code instead of from one auction deadline model.
**Prevention strategy:**
- Define one canonical deadline model from incumbent Go behavior, including bidder `tmax` deductions and upstream response budget.
- Propagate explicit deadlines to every async boundary: hooks, bidder HTTP, cache calls, and background joins.
- Run latency-injected shadow tests and diff `ext.responsetimemillis`, bidder errors, and seat-non-bid status.
**Which phase should address it:** **Phase 3: Auction Core, Deadlines, and Policy Surfaces**

### Pitfall 5: Treating Adapter Parity as Sufficient While Shared Auction Policy Still Differs
**What goes wrong:** Teams port bidder adapters and declare success, but most user-visible behavior differences come from shared auction logic: floors, privacy, currency, category mapping, bid validation, caching, targeting, and response rejection.
**Why it happens:** Adapter packages are the obvious unit of work, but mature Prebid behavior is heavily centralized in exchange and endpoint layers.
**Consequences:** Adapter unit tests pass while full-auction snapshots still diverge.
**Warning signs:**
- Per-adapter tests pass but `/openrtb2/auction` snapshots differ.
- Debug ext, seat-non-bid, or bidder error output mismatches outnumber adapter payload mismatches.
- Policy logic is being reinterpreted while the shared Rust exchange layer is still incomplete.
**Prevention strategy:**
- Prioritize exchange-level regression suites before scaling adapter port volume.
- Separate "adapter HTTP parity" from "auction policy parity" in dashboards and ownership.
- Use stored bid responses and mock bidder servers to isolate floors, privacy, category mapping, and creative validation from network behavior.
**Which phase should address it:** **Phase 3: Auction Core, Deadlines, and Policy Surfaces**

### Pitfall 6: Underestimating Hook and Module Execution-Plan Compatibility
**What goes wrong:** The Rust port focuses on the auction core and postpones hooks/modules, but existing host behavior depends on execution plans, stage ordering, dependencies, account overrides, and per-stage time budgets.
**Why it happens:** Modules look optional from the codebase perspective even though production behavior may depend on them.
**Consequences:** Requests mutate at the wrong stage, module A/B behavior changes, privacy or correction modules stop firing, and operator configs become non-portable.
**Warning signs:**
- Hook execution is stubbed or flattened into one generic middleware stage.
- Account-level execution plans are not represented.
- Module timeouts and dependencies are not modeled explicitly.
**Prevention strategy:**
- Port the execution-plan model before porting individual modules.
- Diff stage-by-stage request/response mutations against Go for representative module stacks.
- Add parity tests for host-level vs account-level plans, dependencies, and timeout exhaustion.
**Which phase should address it:** **Phase 4: Hooks, Modules, and Adapter Integration**

### Pitfall 7: Breaking Config Compatibility With a Cleaner Rust Loader
**What goes wrong:** Rust startup code "improves" config loading, but the deployed Go server already has real semantics for precedence, defaults, case handling, file discovery, account defaults, and static asset loading.
**Why it happens:** Teams treat config as a fresh Rust subsystem instead of a compatibility surface.
**Consequences:** Same deployment config boots differently, env overrides stop working, bidder metadata resolution changes, or operators need migration work before parity exists.
**Warning signs:**
- Rust config code is designed from scratch without a Go precedence matrix.
- Startup only tests one sample config file instead of real config trees plus env/flag combinations.
- Static bidder-info, bidder-params, or category-mapping assets are loaded through a new shape or directory contract.
**Prevention strategy:**
- Write a startup compatibility matrix covering flags, env, file config, defaults, and asset directories.
- Add golden tests for real operator config trees and account-default merges.
- Verify `/info`-style and startup-visible outputs, not just internal structs.
**Which phase should address it:** **Phase 2: Config, Request Parsing, and Merge Semantics**

### Pitfall 8: Ignoring Warm vs Cold Cache Behavior
**What goes wrong:** The Rust server matches steady-state behavior but not first-request, refresh-failure, or post-restart behavior for stored requests, accounts, categories, floors, currency, or rules.
**Why it happens:** Ports usually start with the happy path and postpone background refresh loops and partial-failure semantics.
**Consequences:** Shadow parity looks fine in warmed tests but breaks after deploys, cache misses, backend outages, or config refreshes.
**Warning signs:**
- Mismatches spike only immediately after process start or backend failure.
- Cache wrappers exist in Go but have no behavior-parity tests in Rust.
- Refresh timing, stale-data fallback, or multi-backend semantics are not explicitly designed.
**Prevention strategy:**
- Make cold start, warm cache, stale cache, refresh failure, and multi-backend cases first-class parity scenarios.
- Reproduce Go background task intervals, retry/fallback logic, and partial availability behavior before optimization work.
- Include restart and dependency-outage scenarios in the shadow test plan.
**Which phase should address it:** **Phase 5: Cache, Background Refresh, and Cold-Start Semantics**

### Pitfall 9: Missing Wire Compatibility Outside the Main Auction Body
**What goes wrong:** Teams compare JSON bids but not the full external contract: endpoints, status codes, headers, compression, legacy routes, cookie-sync behavior, AMP/video transforms, and admin/info outputs.
**Why it happens:** Auction-body diffs are easier to automate than end-to-end HTTP compatibility.
**Consequences:** Integrations break even when core auction logic is close, and operators lose trust because probes and tooling do not match.
**Warning signs:**
- Test suites focus on `/openrtb2/auction` but skip `/openrtb2/amp`, `/openrtb2/video`, `/cookie_sync`, `/info`, and admin endpoints used operationally.
- No header/status diffing in parity tests.
- Response-body parity is tracked, but compression and content-type behavior are not.
**Prevention strategy:**
- Add endpoint contract tests for every externally used route, not just auctions.
- Diff status, headers, body, and compression behavior.
- Include admin/info endpoints that SREs rely on during rollout.
**Which phase should address it:** **Phase 6: Wire Compatibility and Shadow Rollout**

### Pitfall 10: Rolling Out Without Comparison-Grade Observability
**What goes wrong:** The team waits to add shadowing and mismatch telemetry until the Rust server is "mostly done," which makes parity gaps expensive to localize.
**Why it happens:** Observability work feels secondary to implementation, but without it the replacement effort becomes guesswork.
**Consequences:** Bugs are discovered from revenue or ops regressions instead of controlled diffing, and the roadmap cannot prioritize the highest-value gaps.
**Warning signs:**
- No request-level Go-vs-Rust diff feed.
- No shared taxonomy for mismatch categories such as parse, config, timeout, module, adapter, or response shaping.
- Rust metrics/logs are not mapped to incumbent operator dashboards.
**Prevention strategy:**
- Ship shadow mode early, with sampled request/response diffing and stable mismatch categories.
- Make "can we compare this against Go in production-like traffic?" a phase gate.
- Reuse incumbent dashboards and alert semantics wherever possible so rollout comparisons are direct.
**Which phase should address it:** **Phase 6: Wire Compatibility and Shadow Rollout**

## Moderate Pitfalls

### Pitfall 1: Over-Porting Go Package Shape Into Rust
**What goes wrong:** The Rust code mirrors the largest Go files and packages 1:1, preserving coupling instead of preserving behavior.
**Warning signs:**
- One Rust module starts to absorb parsing, privacy, hooks, validation, and response shaping.
- New Rust tests are broad integration tests only, with poor fault isolation.
**Prevention strategy:**
- Keep parity at the boundary, not necessarily at the file layout.
- Split Rust code along behavior seams that make parity diffing easier: bootstrap, raw request transforms, typed validation, auction policy, adapter I/O, response shaping.
**Which phase should address it:** **Phase 1** and **Phase 3**

### Pitfall 2: Treating Known Go Bugs as Undefined Behavior
**What goes wrong:** Teams assume incumbent bugs are safe to ignore because they are "obviously wrong," but operators may rely on the current behavior or at least on its failure mode.
**Warning signs:**
- Existing Go concern documents are not turned into explicit parity decisions.
- Rust implementation silently fixes composition, cache, or error-surfacing behavior without a compatibility note.
**Prevention strategy:**
- Classify each known Go bug as one of: must match for cutover, can diverge behind a flag, or can be fixed only after replacement.
- Capture the decision in the roadmap instead of letting it happen incidentally during the port.
**Which phase should address it:** **Phase 1: Parity Oracle and Diff Harness**

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Phase 1: Parity oracle | Building Rust before a Go-vs-Rust diff harness exists | Make corpus capture and diff tooling the first deliverable |
| Phase 2: Config and parsing | Early typed decoding changes merge semantics | Preserve a raw JSON stage and add null/missing/duplicate-key fixtures |
| Phase 2: Bootstrap compatibility | Rust config loader changes precedence/default behavior | Write a precedence matrix from incumbent Go behavior and test it |
| Phase 3: Auction core | Timeout math or cancellation differs under load | Use latency-injected comparison tests and explicit deadline propagation |
| Phase 3: Shared policy | Adapter work hides exchange-level mismatches | Gate progress on full-auction diffs, not adapter unit coverage |
| Phase 4: Hooks/modules | Execution plan order and stage semantics drift | Port plan representation before module bodies and diff per-stage mutations |
| Phase 5: Caches/refresh | Warm tests pass while cold/restart behavior diverges | Add restart, outage, stale-cache, and multi-backend scenarios to parity gates |
| Phase 6: Rollout | No shadow telemetry or operator-comparable dashboards | Ship sampled diffing and metric mapping before traffic cutover |

## Sources

- Internal project context: `/Users/aryehlev/Documents/prebid-server/.planning/PROJECT.md`
- Internal architecture and risk context: `/Users/aryehlev/Documents/prebid-server/.planning/codebase/ARCHITECTURE.md`
- Internal concern inventory: `/Users/aryehlev/Documents/prebid-server/.planning/codebase/CONCERNS.md`
- Internal test-shape context: `/Users/aryehlev/Documents/prebid-server/.planning/codebase/TESTING.md`
- Go `encoding/json` docs: https://pkg.go.dev/encoding/json
- Go `context` docs: https://pkg.go.dev/context
- Serde field attributes: https://serde.rs/field-attrs.html
- Serde container attributes: https://serde.rs/container-attrs.html
- Viper precedence and config behavior: https://github.com/spf13/viper
- Prebid Server auction endpoint docs: https://docs.prebid.org/prebid-server/endpoints/openrtb2/pbs-endpoint-auction
- Prebid Server modules and execution-plan docs: https://docs.prebid.org/prebid-server/pbs-modules/
- JSON Merge Patch semantics: https://datatracker.ietf.org/doc/html/rfc7396
- Rust `HashMap` behavior docs: https://doc.rust-lang.org/std/collections/struct.HashMap.html
- Reqwest client builder docs: https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html
- Tokio timeout docs: https://docs.rs/tokio/latest/tokio/time/fn.timeout.html
