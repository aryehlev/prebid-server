# Codebase Concerns

**Analysis Date:** 2026-04-05

## Tech Debt

**Auction request pipeline is concentrated in a few very large hot-path files:**
- Issue: Request parsing, stored-request merging, privacy handling, hook execution, validation, and response shaping are coupled inside `endpoints/openrtb2/auction.go` (2089 lines), `exchange/exchange.go` (1645 lines), `openrtb_ext/request_wrapper.go` (1878 lines), and `config/config.go` (1425 lines).
- Files: `endpoints/openrtb2/auction.go`, `exchange/exchange.go`, `openrtb_ext/request_wrapper.go`, `config/config.go`
- Impact: Small changes can cross parsing, validation, hooks, analytics, and exchange behavior at once. This increases regression risk and makes hot-path optimization difficult.
- Fix approach: Split by responsibility first, not by package vanity. Extract request-body parsing, stored-request resolution, privacy derivation, and response-ext shaping into smaller units with focused tests before any feature work in these paths.

**Response shaping still carries transitional JSON round-trips:**
- Issue: `setSeatNonBidRaw` explicitly unmarshals and re-marshals `response.Ext` as a transitional step after the exchange has already built the response.
- Files: `endpoints/openrtb2/auction.go`
- Impact: Extra allocations and serialization work occur on the auction response path, and the comment in `endpoints/openrtb2/auction.go` marks the code as temporary rather than settled design.
- Fix approach: Move `SeatNonBid` construction into the main response-building flow and stop treating `response.Ext` as an opaque blob that needs a second pass.

**Rules engine behavior is incomplete and under-instrumented:**
- Issue: The rules engine still contains unresolved behavior and observability TODOs: cold-cache requests are skipped with ambiguous reject behavior, unsupported stages/build errors are silently dropped, result function names are still case-sensitive despite the comment saying otherwise, and rule execution errors are not clearly classified.
- Files: `modules/prebid/rulesengine/module.go`, `modules/prebid/rulesengine/cache_entry.go`, `modules/prebid/rulesengine/result_functions.go`, `modules/prebid/rulesengine/hook_processed_auction.go`, `modules/prebid/rulesengine/observer.go`
- Impact: First-request behavior is nondeterministic for newly seen accounts, operator visibility is weak, and configuration mistakes are easier to miss in production.
- Fix approach: Make cold-start behavior explicit, emit metrics/logs for dropped rule sets and build failures, normalize function names before dispatch, and add tests for cache-miss and invalid-config paths.

## Known Bugs

**Cached category fetcher is stubbed and drops category mapping when in-memory cache is enabled:**
- Symptoms: `fetcherWithCache.FetchCategories` returns `""` and `nil` instead of delegating to the wrapped fetcher.
- Files: `stored_requests/fetcher.go`, `stored_requests/config/config.go`, `exchange/exchange.go`
- Trigger: Any deployment that enables stored-request in-memory cache via `stored_requests.WithCache(...)` and also uses bidder category mapping through `exchange.applyCategoryMapping(...)`.
- Workaround: Disable the cache layer for the affected stored-request configuration or bypass the cached wrapper for category fetching.
- Impact: Category mapping can silently fail while looking successful to the caller because the error value is `nil`.

**Stored response composition is broken when multiple backends are configured:**
- Symptoms: `MultiFetcher.FetchResponses` is a stub that always returns `nil, nil`.
- Files: `stored_requests/multifetcher.go`, `stored_requests/config/config.go`, `stored_responses/stored_responses.go`
- Trigger: Any `config.StoredRequests` data type that resolves to multiple backends through `consolidate(...)`, especially stored responses.
- Workaround: Use a single backend for stored responses until `MultiFetcher.FetchResponses` is implemented.
- Impact: Stored responses can be silently unavailable in multi-backend setups even when source backends contain valid data.

**Category fetch composition hides backend errors:**
- Symptoms: `MultiFetcher.FetchCategories` discards backend errors and rewrites all failures as a generic `NotFoundError`.
- Files: `stored_requests/multifetcher.go`
- Trigger: Category mapping failures from HTTP/file/database backends in a multi-fetcher configuration.
- Workaround: Debug the underlying fetcher directly instead of trusting the surfaced error.
- Impact: Operators lose the real cause of failures such as transport errors, malformed JSON, or backend-specific issues.

**Aidem bidder schema coverage is knowingly incomplete:**
- Symptoms: The test file itself states that it intends to validate `static/bidder-params/aidem.json`, but that schema is still marked as TODO.
- Files: `adapters/aidem/params_test.go`
- Trigger: Changes to Aidem bidder params or schema validation behavior.
- Workaround: Validate Aidem params manually until the missing schema path is added.
- Impact: Bidder parameter validation can drift from test expectations without a matching schema contract.

## Security Considerations

**MySQL TLS uses custom cert validation without hostname verification:**
- Risk: The MySQL provider sets `InsecureSkipVerify: true` and validates only against the supplied root pool, without binding validation to an expected server name.
- Files: `stored_requests/backends/db_provider/mysql_dbprovider.go`
- Current mitigation: Custom `VerifyPeerCertificate` checks certificate trust against the configured root CA.
- Recommendations: Add hostname verification through `ServerName` or equivalent explicit SAN/CN validation and keep the custom root CA handling.

**Stored-requests cache event API is a write-capable endpoint with no built-in authentication and unbounded body reads:**
- Risk: `NewEventsAPI` explicitly documents that the endpoint should not be public without authentication, but `newEventsAPI(...)` registers it directly and the handler reads request bodies with `io.ReadAll`.
- Files: `stored_requests/events/api/api.go`, `stored_requests/config/config.go`
- Current mitigation: Documentation comment only; protection depends on deployment/network boundaries.
- Recommendations: Add authn/authz at the handler or router layer, enforce a request-body limit, and treat the endpoint as admin-only infrastructure.

**Cookie sync request parsing reads the full body without a size cap:**
- Risk: `cookieSyncEndpoint.parseRequest` reads `r.Body` with `io.ReadAll` and does not apply a limit like the OpenRTB auction endpoints do.
- Files: `endpoints/cookie_sync.go`
- Current mitigation: JSON validation happens after the full body is already in memory.
- Recommendations: Wrap the body with `http.MaxBytesReader` or `io.LimitedReader` using a host-configured limit before reading.

**Stored-request HTTP fetchers and event refreshers read entire remote payloads into memory without explicit caps:**
- Risk: Remote HTTP responses are fully read with `io.ReadAll`, including account fetches, category fetches, stored request fetches, and cache refresh payloads.
- Files: `stored_requests/backends/http_fetcher/fetcher.go`, `stored_requests/events/http/http.go`
- Current mitigation: Context timeouts exist for some call paths, but body size is not bounded.
- Recommendations: Add max response-size enforcement before `io.ReadAll` and reject oversized upstream payloads explicitly.

## Performance Bottlenecks

**Auction request processing repeatedly parses and rewrites JSON on the main path:**
- Problem: The request body is read into memory, passed through hook stages, reparsed after hook mutations, merged with stored requests, unmarshalled into `openrtb2.BidRequest`, normalized, and then modified again for bidder params.
- Files: `endpoints/openrtb2/auction.go`, `openrtb_ext/request_wrapper.go`
- Cause: The pipeline mixes raw JSON mutation stages and typed-object mutation stages in one flow.
- Improvement path: Establish a single raw-JSON phase and a single typed-object phase, with clear boundaries and fewer conversions between the two.

**Category mapping does per-bid rejection/translation work without complete observability:**
- Problem: The category-mapping loop iterates through all seat bids, performs category lookups, and drops bids on several paths, but key failure branches still have TODO metrics.
- Files: `exchange/exchange.go`
- Cause: Rejection-heavy logic in `applyCategoryMapping(...)` is on the main auction path and is only partially instrumented.
- Improvement path: Add metrics for missing/invalid categories and mapping misses, then use that data to decide whether caching, batching, or early pruning is warranted.

**Price floor fetching retains per-URL state indefinitely:**
- Problem: `fetchInProgress` is populated for each unique URL and never cleared.
- Files: `floors/fetcher.go`
- Cause: The map is used as a dedupe set for initial fetches, but entries are never deleted after completion.
- Improvement path: Delete entries when work completes or replace the map with an expiring dedupe structure keyed by URL.

## Fragile Areas

**OpenRTB auction path is fragile because too many concerns meet in one file:**
- Files: `endpoints/openrtb2/auction.go`
- Why fragile: Request compression, stored requests, account lookup, hooks, privacy, validation, bidder param merging, and response shaping all live in one orchestration file.
- Safe modification: Isolate one responsibility at a time behind tests before editing shared control flow.
- Test coverage: `endpoints/openrtb2/auction_test.go` is large, but its breadth makes failures harder to localize and does not replace smaller focused unit tests.

**Exchange execution is fragile because it mixes bidder orchestration with policy and targeting rules:**
- Files: `exchange/exchange.go`
- Why fragile: Auction orchestration, bidder fan-out, privacy handling, price floors, category mapping, deduplication, and cache/debug behavior all converge here.
- Safe modification: Change one policy surface at a time and verify exchange tests plus endpoint integration tests together.
- Test coverage: `exchange/exchange_test.go` is extensive, but the file size signals high coupling and a large blast radius for edits.

**Stored-request fetcher composition is fragile because interface methods are only partially implemented:**
- Files: `stored_requests/fetcher.go`, `stored_requests/multifetcher.go`, `stored_requests/config/config.go`
- Why fragile: `AllFetcher` composition looks generic, but some methods are stubbed or partially delegated. Bugs in rarely used combinations can survive unnoticed.
- Safe modification: Add interface-conformance tests that exercise every `AllFetcher` method through cached and multi-fetcher compositions before refactoring.
- Test coverage: `stored_requests/fetcher_test.go` exists, but there are no tests covering cached `FetchCategories` or multi-fetcher `FetchResponses` / `FetchCategories`.

**Rules engine startup path is fragile because behavior changes across cache state:**
- Files: `modules/prebid/rulesengine/module.go`, `modules/prebid/rulesengine/cache_entry.go`, `modules/prebid/rulesengine/tree_manager.go`
- Why fragile: The first request for an account can enqueue async rule building and skip enforcement, while later requests take a different path.
- Safe modification: Treat cold-cache, rebuild, and invalid-config behavior as separate test cases and keep request-time behavior deterministic.
- Test coverage: There are rule-engine tests, but the unresolved TODOs show important startup/error semantics are still unsettled.

## Scaling Limits

**Price floor dynamic-fetch state grows with unique configured URLs:**
- Current capacity: Bounded worker pool and queue, but unbounded `fetchInProgress` cardinality.
- Limit: High-cardinality per-account floor URLs will cause process memory growth over time.
- Scaling path: Normalize floor fetch URLs per shared source when possible, or evict completed URLs from `fetchInProgress`.

**Monolithic hot-path files limit parallel development and safe review throughput:**
- Current capacity: Core request behavior is understandable only through a small set of very large files.
- Limit: Team velocity drops as more features land in `endpoints/openrtb2/auction.go`, `exchange/exchange.go`, and `openrtb_ext/request_wrapper.go`.
- Scaling path: Split orchestration from policy logic and require smaller, responsibility-based files for new work.

## Dependencies at Risk

**Analytics HTTP client defaults rely on global client behavior and an unresolved TODO:**
- Risk: `analytics/build/build.go` injects `analytics/clients/http.go` which returns `http.DefaultClient`, and the TODO in that helper is still unresolved.
- Impact: Pubstack config fetches and event sends in `analytics/pubstack/config.go` and `analytics/pubstack/eventchannel/sender.go` inherit whatever timeout/transport behavior the process default client has.
- Migration plan: Replace the shared default client with an explicitly configured analytics client and add package-level tests around timeout behavior.

## Missing Critical Features

**No complete contract for cached or multi-source category/stored-response fetching:**
- Problem: Composition helpers exist, but `FetchCategories` and `FetchResponses` are not fully implemented across wrappers.
- Blocks: Reliable use of category mapping and stored responses when cache layers or multiple backends are enabled.

**Aidem bidder schema artifact is still missing:**
- Problem: The params test explicitly calls out the missing `static/bidder-params/aidem.json` contract.
- Blocks: Trustworthy schema validation for the Aidem adapter.

## Test Coverage Gaps

**Fetcher composition methods are not protected by regression tests:**
- What's not tested: Cached `FetchCategories` and multi-fetcher `FetchResponses` / `FetchCategories`.
- Files: `stored_requests/fetcher.go`, `stored_requests/multifetcher.go`, `stored_requests/fetcher_test.go`, `stored_requests/multifetcher_test.go`
- Risk: Interface stubs and silent delegation bugs can ship even though the surrounding fetcher code appears covered.
- Priority: High

**Rules engine cold-start and error-classification paths are not locked down by behavior-focused tests:**
- What's not tested: First-request cache miss behavior, unsupported-stage handling visibility, error classification, and case-insensitive function lookup.
- Files: `modules/prebid/rulesengine/module.go`, `modules/prebid/rulesengine/cache_entry.go`, `modules/prebid/rulesengine/result_functions.go`, `modules/prebid/rulesengine/hook_processed_auction.go`
- Risk: Production behavior can differ between warm and cold accounts, and configuration mistakes may stay silent.
- Priority: High

**Request-size enforcement is inconsistent across endpoints and lacks dedicated guardrail tests:**
- What's not tested: Maximum request/body size behavior for `cookie_sync` and stored-request admin endpoints.
- Files: `endpoints/cookie_sync.go`, `stored_requests/events/api/api.go`
- Risk: Oversized payload regressions can become memory-pressure or abuse issues without failing tests.
- Priority: Medium

**Analytics client timeout behavior is not directly covered in the package that chooses the client:**
- What's not tested: The default-client selection in `analytics/clients/http.go` and its effect on Pubstack networking paths.
- Files: `analytics/clients/http.go`, `analytics/build/build.go`, `analytics/pubstack/config.go`, `analytics/pubstack/eventchannel/sender.go`
- Risk: Network stalls or transport regressions can surface only in integrated runtime behavior.
- Priority: Medium

---

*Concerns audit: 2026-04-05*
