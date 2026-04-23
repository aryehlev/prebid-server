# Architecture Patterns

**Domain:** Rust Prebid Server parity port
**Researched:** 2026-04-05
**Confidence:** MEDIUM-HIGH

## Recommended Architecture

The Rust implementation should preserve the Go server's architecture, not replace it with a cleaner-but-different design. The Go server already has the right macro-shape for parity: a composition root that wires long-lived services, transport endpoints that normalize requests, a central auction core, strict adapter boundaries, a hook/module execution layer, and support services for cache, stored data, privacy, analytics, and metrics. The Rust port should mirror those seams directly.

Use the current Rust workspace as the base, but tighten the boundaries:

1. `server` stays the composition root only.
2. `config` becomes the typed compatibility layer for host config, account config, hook execution plans, bidder metadata, bidder params, and env/file loading.
3. `endpoints` owns HTTP parsing, request normalization, response writing, and endpoint-specific flows only.
4. `exchange` owns deterministic auction orchestration only.
5. `adapters` owns bidder protocol translation only.
6. `cache` owns the Prebid Cache client only.
7. Add or extract `stored-data`, `hooks`, and `modules` as first-class crates or subdomains rather than leaving that logic inside `endpoints` or `exchange`.

The rule is simple: if a behavior exists in Go as a stable seam, keep that seam in Rust. Do not collapse stages together just because async Rust makes it easy.

## Current Divergences To Correct First

These are the architectural mismatches most likely to cause parity drift:

- Rust hooks currently define seven stages but omit `exitpoint`, while Go and official module docs include it.
- Rust exchange currently wires only `EntrypointRaw` and `AllProcessedBidResponses`; Go executes the full stage set around request parsing, bidder fan-out, response assembly, and exit.
- Rust config currently models cache and stored requests, but not the Go hook/module configuration surface (`modules`, `host_execution_plan`, `default_account_execution_plan`).
- Rust `endpoints` currently owns stored-request loading and some stored-response short-circuit behavior that should sit in a reusable stored-data layer.
- Rust `server` currently parses bidder YAML and usersync fields manually, which is brittle for config compatibility.
- Rust has two cache abstractions (`rust/crates/cache` and `rust/crates/exchange/src/cache.rs`), which will drift.

## Component Boundaries

| Component | Responsibility | Must Not Own | Communicates With |
|-----------|---------------|--------------|-------------------|
| `server` / bootstrap | Load config and static assets, build clients, register routes, start listeners, own shutdown | Auction logic, hook logic, YAML scraping heuristics | `config`, `stored-data`, `modules`, `hooks`, `adapters`, `exchange`, `endpoints`, `metrics`, `analytics` |
| `config` | Parse host config, account config, hook plans, bidder metadata, bidder params, defaults, aliases, feature flags | HTTP handlers, bidder calls, cache puts | All runtime components |
| `stored-data` | Resolve stored requests, stored imps, stored responses, accounts, category mappings, cache invalidation events | HTTP routing, bidder logic | `config`, `endpoints`, `exchange` |
| `hooks` | Hook traits, repository, merged host/account execution plans, stage executor, outcomes, module context carry-forward | Module registry, endpoint parsing, bidder protocol logic | `config`, `modules`, `exchange`, `endpoints`, `metrics` |
| `modules` | Built-in module registration and initialization with explicit dependencies | Core hook scheduling, request parsing | `hooks`, `config`, `metrics`, `analytics`, HTTP clients |
| `adapters` | One bidder boundary per adapter: validate bidder ext, build outbound requests, parse bidder responses | Auction orchestration, cache writes, hook scheduling | `exchange`, static bidder metadata, shared OpenRTB/ext types |
| `exchange` | Deterministic auction kernel: request splitting, bidder fan-out, timeouts, validation, floors, privacy gating, targeting, seat non-bids, cache decisions, final OpenRTB response assembly | HTTP parsing, config file I/O, module registry | `adapters`, `hooks`, `cache`, `metrics`, `analytics`, `privacy`, `currency`, `stored-data` |
| `endpoints` | Endpoint-specific request parsing and normalization for `/openrtb2/auction`, `/amp`, `/video`, `/setuid`, `/cookie_sync`, `/event`, `/vtrack`, `/info/*` | Cross-bidder auction logic | `exchange`, `stored-data`, `hooks`, `config`, `metrics`, `analytics` |
| `cache` | Typed client for Prebid Cache `/cache` API and ext-cache URL construction | Auction policy, endpoint logic | `exchange`, `endpoints/vtrack` |

## Required Hook And Module Shape

Rust should match the Go hook lifecycle exactly:

1. `entrypoint`
2. `raw_auction_request`
3. `processed_auction_request`
4. `bidder_request`
5. `raw_bidder_response`
6. `all_processed_bid_responses`
7. `auction_response`
8. `exitpoint`

Key implementation rule: the hook planner must merge `host_execution_plan` with account-level execution plans the same way Go does, preserving group order, timeout semantics, rejectability, module context carry-forward, and per-hook codes. A generic "all hooks in one group with a default timeout" planner is not sufficient for parity.

Modules should stay behind a repository plus explicit builder layer, just like Go. That keeps three things stable during the port:

- startup-only initialization config
- per-account runtime config
- stage-specific hook interfaces with typed payloads

## Adapter Boundary

The adapter contract should stay narrow and dumb:

- input: already-normalized bidder request plus bidder-specific context
- output: outbound HTTP requests and parsed bidder responses
- no account lookup
- no stored request resolution
- no cache writes
- no module scheduling

This is the main protection against parity drift while hundreds of adapters are still being ported. If the exchange kernel decides auction policy and adapters only translate protocol, adapter ports can be validated one-by-one without re-auditing the whole server.

Do not rely on generic fallback adapters for production parity. They are acceptable only as temporary scaffolding for compile coverage, not as a compatibility strategy.

## Data Flow

### Startup

1. `server` loads bidder info, bidder params, category maps, and host config through typed `config` loaders.
2. `server` builds shared clients and services: HTTP clients, metrics, analytics, currency, cache, stored-data backends, privacy helpers.
3. `modules` initializes enabled modules from host config and returns a hook repository plus shutdown handles.
4. `hooks` builds the execution-plan builder from host config and account-plan defaults.
5. `adapters` builds the active bidder map from config and bidder metadata.
6. `exchange` is constructed from adapters plus support services.
7. `endpoints` are constructed from `exchange`, `stored-data`, `hooks`, `config`, and response writers.

### `/openrtb2/auction`

1. `endpoints` reads the raw HTTP request and runs the `entrypoint` stage.
2. `endpoints` resolves stored requests, stored responses, account config, aliases, defaults, and endpoint-specific validation.
3. `endpoints` runs `raw_auction_request`, then constructs the canonical auction request.
4. `endpoints` runs `processed_auction_request` after stored-request merge and all request enrichments.
5. `exchange` splits the request per bidder, running `bidder_request` before each outbound call.
6. `exchange` receives bidder responses and runs `raw_bidder_response` per bidder.
7. `exchange` performs core bid validation, privacy enforcement, floors, currency, targeting, seat non-bids, cache decisions, and response assembly.
8. `exchange` runs `all_processed_bid_responses`.
9. `endpoints` runs `auction_response`, enriches `response.ext.prebid.modules`, then runs `exitpoint`.
10. `endpoints` writes the final HTTP response.

### `/openrtb2/amp` and `/openrtb2/video`

Keep the Go pattern: these endpoints perform transport-specific request shaping, then call the same auction core. Do not fork auction logic into endpoint-specific branches.

## Suggested Build Order

Build in dependency order, not by perceived business value:

1. **Config and static metadata compatibility**
   Reason: every other layer depends on exact bidder-info, params, aliases, defaults, and hook plan semantics.
2. **Stored-data and account resolution**
   Reason: request normalization is not stable until stored requests, accounts, and stored responses behave like Go.
3. **Hooks and modules framework**
   Reason: auction behavior is already shaped by module execution points; porting exchange without them will create rework.
4. **Shared support services**
   Reason: unify cache, currency, privacy, analytics, and metrics contracts before broad endpoint work.
5. **Exchange kernel parity**
   Reason: once request normalization and hook surfaces are stable, the core auction flow can be made deterministic.
6. **Endpoint parity**
   Reason: `/auction`, `/amp`, and `/video` should become thin shells over a stable kernel.
7. **Adapter completion**
   Reason: adapter ports are safer once the kernel and pre/post-processing seams are fixed.
8. **Operational and long-tail endpoints**
   Reason: `cookie_sync`, `setuid`, `event`, `vtrack`, admin, and info endpoints depend on the same config/runtime surfaces but should not drive architecture.

## How To Prove Parity Incrementally

The parity strategy should be layered and executable after each step:

### 1. Lock Go Behavior As The Oracle

Reuse the existing Go fixtures and tests as the parity corpus:

- `endpoints/openrtb2/sample-requests/**`
- `endpoints/openrtb2/auction_test.go`
- `endpoints/openrtb2/amp_auction_test.go`
- `endpoints/openrtb2/video_auction_test.go`
- `exchange/exchange_test.go`

### 2. Build A Cross-Language Parity Harness

For each fixture, run the Go and Rust servers with the same config and compare:

- HTTP status
- normalized request after stored-request merge
- bidder request payloads per adapter
- bidder response handling
- final OpenRTB response body
- `response.ext.prebid` warnings/errors/modules
- targeting keys
- seat non-bids
- cache side effects and cache URL fields

Normalize only known nondeterministic fields:

- generated UUIDs
- timestamps
- request IDs when config says they may be generated
- debug timing fields

Everything else should compare byte-for-byte or via canonical JSON.

### 3. Add Stage-Level Snapshots

Do not compare only the final response. Capture and compare intermediate artifacts at the same architectural seams:

- raw body after `entrypoint`
- canonical request after stored-request merge
- request after `processed_auction_request`
- per-bidder outbound JSON after `bidder_request`
- per-bidder parsed response after `raw_bidder_response`
- final response after `auction_response`
- final HTTP body after `exitpoint`

This is how you detect drift before it becomes a visible auction mismatch.

### 4. Port By Slice, Not By Big Bang

Recommended slice order:

1. one canonical bidder path through `/openrtb2/auction`
2. stored requests and account merge
3. hooks/modules planner with a no-op module and one mutating module
4. cache-integrated banner path
5. video and AMP transforms
6. adapter-by-adapter ports

### 5. Keep A Compatibility Matrix

Track parity status separately for:

- config fields
- hook stages
- module runtime config
- stored request backends
- cache behaviors
- endpoints
- adapters

This prevents "works for one golden request" from being mistaken for production replacement parity.

## Anti-Patterns To Avoid

### Anti-Pattern 1: Reinterpreting Go behavior into a new Rust domain model

**Why bad:** parity bugs will look like design improvements.
**Instead:** preserve Go request stages, contracts, and merge semantics even when the Rust representation is cleaner internally.

### Anti-Pattern 2: Mixing transport and auction logic

**Why bad:** `/auction`, `/amp`, and `/video` will drift from each other and from Go.
**Instead:** endpoints normalize input; exchange runs the auction.

### Anti-Pattern 3: Treating hooks as optional decoration

**Why bad:** modules already change behavior, not just observability.
**Instead:** make the hook planner and executor a first-class dependency before finishing core parity.

### Anti-Pattern 4: Manual YAML scraping for bidder info compatibility

**Why bad:** bidder metadata is operational behavior, not best-effort decoration.
**Instead:** load bidder info and params with typed loaders that preserve Go semantics.

### Anti-Pattern 5: Duplicating support-service clients

**Why bad:** cache, metrics, and stored-data drift will show up as subtle response differences.
**Instead:** one crate and one contract per support service.

## Roadmap Implications

The roadmap should treat architecture stabilization as parity work, not cleanup work. The first architecture phases should be:

1. make config/static loading Go-compatible
2. extract stored-data, hooks, and modules into stable seams
3. wire the full hook lifecycle including `exitpoint`
4. unify cache integration around one client contract
5. only then widen exchange and adapter parity

If the port continues without those seams, later adapter and module work will be forced to backtrack.

## Sources

### Repository Sources

- `main.go`
- `router/router.go`
- `config/hooks.go`
- `hooks/plan.go`
- `hooks/hookexecution/executor.go`
- `endpoints/openrtb2/auction.go`
- `endpoints/openrtb2/auction_test.go`
- `exchange/exchange.go`
- `exchange/exchange_test.go`
- `rust/crates/server/src/main.rs`
- `rust/crates/config/src/lib.rs`
- `rust/crates/endpoints/src/lib.rs`
- `rust/crates/endpoints/src/stored_requests.rs`
- `rust/crates/exchange/src/lib.rs`
- `rust/crates/exchange/src/hooks.rs`
- `rust/crates/cache/src/lib.rs`
- `rust/crates/exchange/src/cache.rs`

### Official Sources

- Prebid Server overview: https://docs.prebid.org/prebid-server/overview/prebid-server-overview.html
- Prebid Server module architecture and stages: https://docs.prebid.org/prebid-server/developers/add-a-module.html
- Stored request semantics: https://docs.prebid.org/prebid-server/features/pbs-storedreqs-java.html
- Prebid Cache API and backend expectations: https://github.com/prebid/prebid-cache
