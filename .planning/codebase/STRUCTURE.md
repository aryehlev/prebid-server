# Codebase Structure

**Analysis Date:** 2026-04-05

## Directory Layout

```text
prebid-server/
├── main.go                 # Process entrypoint
├── server/                 # HTTP listener and server lifecycle helpers
├── router/                 # Route registration and transport wrappers
├── endpoints/              # HTTP handlers by API surface
├── exchange/               # Auction orchestration core
├── adapters/               # One bidder package per adapter
├── config/                 # Host configuration types and loading
├── openrtb_ext/            # OpenRTB extension models and bidder ext types
├── stored_requests/        # Stored request fetchers, caches, and event sources
├── hooks/                  # Hook contracts, plans, and execution
├── modules/                # Built-in hook modules and module builder
├── privacy/                # Privacy helpers split by regime
├── analytics/              # Analytics module implementations and builder
├── metrics/                # Metrics engines and backends
├── static/                 # Bidder metadata, schemas, and static assets
├── util/                   # Small shared helper packages by concern
├── rust/                   # Parallel Rust workspace mirroring core domains
└── sample/                 # Deployment and integration examples
```

## Directory Purposes

**`server/`:**
- Purpose: Own listener creation, graceful shutdown, compression, connection metrics, and Prometheus HTTP serving.
- Contains: `server/server.go`, `server/listener.go`, `server/prometheus.go`, `server/ssl/`
- Key files: `server/server.go`, `server/listener.go`, `server/prometheus.go`

**`router/`:**
- Purpose: Compose application dependencies and register every HTTP route.
- Contains: Main router assembly, admin mux, request timeout aspect.
- Key files: `router/router.go`, `router/admin.go`, `router/aspects/request_timeout_handler.go`

**`endpoints/`:**
- Purpose: Keep HTTP handler code grouped by endpoint family.
- Contains: Root handlers like `endpoints/cookie_sync.go`, endpoint subpackages `endpoints/info/`, `endpoints/events/`, `endpoints/openrtb2/`
- Key files: `endpoints/openrtb2/auction.go`, `endpoints/openrtb2/amp_auction.go`, `endpoints/openrtb2/video_auction.go`, `endpoints/setuid.go`

**`exchange/`:**
- Purpose: Hold cross-bidder auction logic and bidder fan-out implementation.
- Contains: Exchange interface and implementation, targeting, price buckets, bidder wrappers, auction response assembly, test fixtures.
- Key files: `exchange/exchange.go`, `exchange/auction.go`, `exchange/bidder.go`, `exchange/adapter_builders.go`

**`adapters/`:**
- Purpose: Isolate bidder-specific request and response translation code.
- Contains: One package per bidder such as `adapters/33across/`, `adapters/appnexus/`, `adapters/rubicon/`, plus shared adapter test utilities in `adapters/adapterstest/`
- Key files: `adapters/33across/33across.go`, `adapters/appnexus/appnexus.go`, `adapters/adapterstest/adapter_test_util.go`

**`config/`:**
- Purpose: Define configuration structs, defaults, validation, bidder-info parsing, and related helpers.
- Contains: `config.Configuration`, account config, events config, hooks config, compression config, test fixtures in `config/test/`
- Key files: `config/config.go`, `config/bidderinfo.go`, `config/account.go`, `config/hooks.go`

**`openrtb_ext/`:**
- Purpose: Store OpenRTB extension models and bidder-specific imp ext structs.
- Contains: Shared extension types plus bidder-specific files like `openrtb_ext/imp_appnexus.go`
- Key files: `openrtb_ext/bidders.go`, `openrtb_ext/alternatebiddercodes.go`, `openrtb_ext/bid_request_video.go`, `openrtb_ext/imp_appnexus.go`

**`stored_requests/`:**
- Purpose: Resolve stored requests, accounts, category mappings, and stored responses from file, DB, or HTTP backends.
- Contains: Backends in `stored_requests/backends/`, caches in `stored_requests/caches/`, orchestration in `stored_requests/config/`, event listeners in `stored_requests/events/`
- Key files: `stored_requests/config/config.go`, `stored_requests/fetcher.go`, `stored_requests/backends/http_fetcher/fetcher.go`

**`hooks/`:**
- Purpose: Define hook stages, repositories, plans, and per-request execution.
- Contains: `hooks/plan.go`, `hooks/repo.go`, stage interfaces in `hooks/hookstage/`, runtime execution in `hooks/hookexecution/`
- Key files: `hooks/plan.go`, `hooks/repo.go`, `hooks/hookexecution/executor.go`

**`modules/`:**
- Purpose: Ship built-in hook modules and the module builder that registers them.
- Contains: Vendor subdirectories such as `modules/prebid/`, `modules/scope3/`, `modules/fiftyonedegrees/`, builder code in `modules/modules.go` and `modules/builder.go`
- Key files: `modules/modules.go`, `modules/builder.go`, `modules/moduledeps/deps.go`

**`privacy/`:**
- Purpose: Group privacy helpers by framework and enforcement need.
- Contains: `privacy/gdpr/`, `privacy/ccpa/`, `privacy/gpp/`, `privacy/lmt/`
- Key files: `privacy/activitycontrol.go`, `privacy/gdpr/consentwriter.go`, `privacy/ccpa/policy.go`, `privacy/lmt/policy.go`

**`analytics/`:**
- Purpose: Provide analytics module implementations and runtime fan-out.
- Contains: Module packages in `analytics/agma/`, `analytics/pubstack/`, `analytics/filesystem/`, and builder in `analytics/build/build.go`
- Key files: `analytics/build/build.go`, `analytics/pubstack/pubstack_module.go`, `analytics/agma/agma_module.go`, `analytics/filesystem/file_module.go`

**`metrics/`:**
- Purpose: Define metrics interfaces and backends.
- Contains: Core metrics types, config-based engine builder, Prometheus backend.
- Key files: `metrics/config/metrics.go`, `metrics/prometheus/prometheus.go`, `metrics/metrics.go`

**`static/`:**
- Purpose: Hold startup-required metadata and static HTTP assets.
- Contains: Bidder metadata YAML in `static/bidder-info/`, bidder param schemas in `static/bidder-params/`, category maps in `static/category-mapping/`, UI assets like `static/index.html`
- Key files: `static/bidder-info/appnexus.yaml`, `static/bidder-params/appnexus.json`, `static/category-mapping/freewheel/freewheel.json`, `static/index.html`

**`util/`:**
- Purpose: Provide small, focused helper packages without pulling in higher-level auction concerns.
- Contains: Subpackages such as `util/jsonutil/`, `util/iputil/`, `util/uuidutil/`, `util/task/`
- Key files: `util/jsonutil/jsonutil.go`, `util/iputil/validator.go`, `util/task/ticker_task.go`

**`rust/`:**
- Purpose: Maintain a separate Rust workspace that mirrors major PBS domains.
- Contains: Workspace manifest `rust/Cargo.toml` and crates under `rust/crates/` such as `exchange`, `endpoints`, `server`, `config`
- Key files: `rust/Cargo.toml`, `rust/crates/exchange/Cargo.toml`, `rust/crates/endpoints/Cargo.toml`

## Key File Locations

**Entry Points:**
- `main.go`: Process startup and high-level dependency bootstrapping.
- `router/router.go`: Main HTTP route registration and composition root.
- `router/admin.go`: Admin-only mux for pprof, currency rates, and version.
- `server/server.go`: Main listener startup and graceful shutdown.

**Configuration:**
- `config/config.go`: `config.Configuration`, Viper defaults, validation, and config loading.
- `config/bidderinfo.go`: Bidder metadata model and disk loading.
- `config/account.go`: Account-related config types.
- `config/hooks.go`: Hook and module configuration types.

**Core Logic:**
- `exchange/exchange.go`: Auction orchestration and `HoldAuction`.
- `exchange/bidder.go`: Bidder request execution, request/response adaptation, throttling, and debug capture.
- `endpoints/openrtb2/auction.go`: Standard OpenRTB handler and request parsing.
- `account/account.go`: Runtime account resolution and derived-account setup.

**Testing:**
- `main_test.go`: Entry-level startup coverage.
- `endpoints/openrtb2/auction_test.go`: End-to-end handler tests for the main auction surface.
- `exchange/exchange_test.go`: High-coverage auction-core behavior tests.
- `adapters/appnexus/appnexus_test.go`: Adapter-specific unit tests living beside implementation files.

## Naming Conventions

**Files:**
- Use lowercase Go filenames with underscores for multiword names: `server/server.go`, `exchange/auction_response.go`, `stored_requests/config/config.go`.
- Keep endpoint handlers named after the endpoint or transport concern: `endpoints/cookie_sync.go`, `endpoints/openrtb2/video_auction.go`, `endpoints/events/vtrack.go`.
- Keep tests colocated with production files and suffix them with `_test.go`: `exchange/bidder_test.go`, `config/config_test.go`, `adapters/aax/aax_test.go`.
- For bidder OpenRTB extension types, add `openrtb_ext/imp_<bidder>.go`: `openrtb_ext/imp_appnexus.go`, `openrtb_ext/imp_rubicon.go`.

**Directories:**
- Core domain packages stay at repository root and use concise singular or domain names: `exchange`, `router`, `currency`, `gdpr`, `usersync`.
- Endpoint subdirectories map to API families: `endpoints/openrtb2/`, `endpoints/info/`, `endpoints/events/`.
- Bidder directories under `adapters/` use the bidder code or established brand identifier exactly as supported by the project: `adapters/33across/`, `adapters/audienceNetwork/`, `adapters/alliance_gravity/`.

## Where to Add New Code

**New Feature:**
- Primary code: Put cross-bidder auction behavior in `exchange/` and wire it from `exchange/exchange.go`.
- Tests: Add colocated tests beside the touched files, usually `*_test.go` in the same package. For request/response samples, reuse `endpoints/openrtb2/sample-requests/` or existing `exchange/*test/` fixture directories.

**New Component/Module:**
- New HTTP endpoint: Implement under `endpoints/` or the appropriate subpackage and register it in `router/router.go`.
- New OpenRTB auction variant: Place transport logic in `endpoints/openrtb2/` and continue to call `exchange.Exchange.HoldAuction` rather than duplicating auction logic.
- New hook module: Add the module package under `modules/<vendor>/<module>/`, then register the builder in `modules/builder.go`.
- New bidder adapter: Add `adapters/<bidder>/<bidder>.go`, adapter tests in the same directory, bidder metadata in `static/bidder-info/<bidder>.yaml`, and bidder ext types in `openrtb_ext/imp_<bidder>.go`. Wire the adapter into `exchange/adapter_builders.go`.

**Utilities:**
- Shared helpers: Add narrow helpers to the relevant `util/<topic>/` package instead of a generic catch-all package.
- Request or model helpers tied to OpenRTB should usually live in `openrtb_ext/`, `ortb/`, or the domain package that owns the behavior.
- Configuration helpers belong in `config/` if they operate on `config.Configuration` or related config structs.

## Special Directories

**`static/`:**
- Purpose: Startup-required metadata and publicly served static assets.
- Generated: No
- Committed: Yes

**`rust/`:**
- Purpose: Separate Rust workspace with crates mirroring server, exchange, endpoints, metrics, cache, and config domains.
- Generated: No
- Committed: Yes

**`sample/`:**
- Purpose: Example deployments and runnable integration samples for learning and experimentation.
- Generated: No
- Committed: Yes

**`.github/workflows/`:**
- Purpose: CI validation workflows for the repository.
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-04-05*
