# Architecture

**Analysis Date:** 2026-04-05

## Pattern Overview

**Overall:** Dependency-assembled Go monolith with layered request handling and a central auction orchestration core.

**Key Characteristics:**
- Startup is centralized in `main.go`, which loads bidder metadata from `static/bidder-info/`, builds a `config.Configuration`, creates the router, and starts the HTTP servers.
- Dependency wiring is concentrated in `router/router.go`; handlers and services receive prebuilt collaborators instead of constructing them lazily.
- Auction execution is split between HTTP-facing endpoint code in `endpoints/openrtb2/*.go` and cross-bidder orchestration in `exchange/exchange.go`.

## Layers

**Bootstrap and Runtime:**
- Purpose: Start the process, load static and dynamic configuration, create background tasks, and own server lifecycle.
- Location: `main.go`, `server/server.go`, `server/listener.go`, `server/prometheus.go`
- Contains: Process entrypoint, config loading, server listeners, graceful shutdown, compression, metrics listener setup.
- Depends on: `config`, `router`, `currency`, `util/task`, `metrics/config`
- Used by: The whole application runtime.

**Configuration and Static Metadata:**
- Purpose: Define host configuration, account defaults, bidder metadata, and runtime feature flags.
- Location: `config/config.go`, `config/bidderinfo.go`, `static/bidder-info/`, `static/bidder-params/`, `static/category-mapping/`
- Contains: `config.Configuration`, bidder YAML parsing, Viper defaults, request validation settings, static bidder parameter schemas.
- Depends on: `viper`, `openrtb_ext`, `logger`, `util/jsonutil`
- Used by: `main.go`, `router/router.go`, `exchange/exchange.go`, `account/account.go`, `usersync`, `stored_requests`

**Transport and Endpoint Layer:**
- Purpose: Translate HTTP requests into internal request objects and marshal responses back to HTTP.
- Location: `router/router.go`, `router/admin.go`, `router/aspects/request_timeout_handler.go`, `endpoints/`, `endpoints/openrtb2/`
- Contains: Route registration, CORS wrapping, admin mux, request-size enforcement, endpoint-specific parsing, response writing.
- Depends on: `config`, `exchange`, `analytics`, `metrics`, `stored_requests`, `hooks/hookexecution`, `privacy`, `usersync`
- Used by: External clients calling `/openrtb2/auction`, `/openrtb2/amp`, `/openrtb2/video`, `/cookie_sync`, `/setuid`, `/event`, `/vtrack`, `/info/*`

**Auction Core:**
- Purpose: Execute one auction across all relevant bidders and assemble the OpenRTB response.
- Location: `exchange/exchange.go`, `exchange/auction.go`, `exchange/bidder.go`, `exchange/auction_response.go`, `exchange/entities/`
- Contains: `exchange.Exchange`, `AuctionRequest`, request splitting, bidder fan-out, bid validation, price floors, category mapping, targeting, caching, seat non-bid handling.
- Depends on: `adapters`, `prebid_cache_client`, `currency`, `privacy`, `gdpr`, `floors`, `macros`, `metrics`, `stored_requests`, `stored_responses`
- Used by: `endpoints/openrtb2/auction.go`, `endpoints/openrtb2/amp_auction.go`, `endpoints/openrtb2/video_auction.go`

**Extensibility and Integration Layer:**
- Purpose: Plug in bidder-specific protocol logic and hook/module-based auction extensions.
- Location: `adapters/`, `exchange/adapter_builders.go`, `modules/modules.go`, `modules/builder.go`, `hooks/plan.go`, `hooks/repo.go`, `hooks/hookexecution/`
- Contains: One adapter package per bidder, generated adapter registration, module builders, hook repository, execution plans, per-stage hook execution.
- Depends on: `config`, `moduledeps`, `hookstage`, `exchange`, `openrtb_ext`
- Used by: `router/router.go` during startup and `exchange/*` during auction execution

**Support Services:**
- Purpose: Provide reusable subsystems around storage, privacy, analytics, metrics, and OpenRTB model extensions.
- Location: `stored_requests/`, `stored_responses/`, `analytics/build/build.go`, `metrics/config/metrics.go`, `privacy/`, `gdpr/`, `openrtb_ext/`, `account/account.go`
- Contains: Stored request fetchers and caches, analytics fan-out, metrics multiplexing, account resolution, privacy enforcement helpers, OpenRTB extension types.
- Depends on: External libraries and package-local helpers.
- Used by: `router/router.go`, `endpoints/openrtb2/*.go`, `exchange/*`, `endpoints/*`

## Data Flow

**Process Startup:**

1. `main.go` resolves `./static/bidder-info`, loads bidder metadata via `config.LoadBidderInfoFromDisk`, and builds `config.Configuration` through `config.SetupViper` and `config.New`.
2. `main.go` creates shared long-lived services such as `currency.NewRateConverter` and passes them into `router.New`.
3. `router/router.go` builds HTTP clients, stored request fetchers, metrics, analytics, modules, hooks, adapters, `exchange.NewExchange`, and all endpoint handlers, then registers routes.
4. `server.Listen` in `server/server.go` starts the main listener, optional admin listener, and optional Prometheus listener and waits for shutdown signals.

**OpenRTB Auction Request:**

1. `router/router.go` routes `POST /openrtb2/auction` to the handler returned by `endpoints/openrtb2.NewEndpoint`.
2. `(*endpointDeps).Auction` in `endpoints/openrtb2/auction.go` parses the HTTP request, resolves stored requests and accounts, applies privacy and hook entry stages, and builds `exchange.AuctionRequest`.
3. `exchange.(*exchange).HoldAuction` in `exchange/exchange.go` executes the processed-auction hook stage, derives bidder requests, fans out to adapters, applies floors and targeting, and builds `openrtb2.BidResponse`.
4. `sendAuctionResponse` in `endpoints/openrtb2/auction.go` runs auction-response and exitpoint hooks, enriches response extensions with hook outcomes, and JSON-encodes the final response.

**Video Auction Request:**

1. `router/router.go` routes `POST /openrtb2/video` to `endpoints/openrtb2.NewVideoEndpoint`.
2. `(*endpointDeps).VideoAuctionEndpoint` in `endpoints/openrtb2/video_auction.go` merges simplified video input with stored video request data, synthesizes OpenRTB impressions, validates the generated request, and calls `exchange.Exchange.HoldAuction`.
3. The exchange returns a standard `exchange.AuctionResponse`, and the video endpoint converts it into the deprecated video response format before writing the response.

**Stored Request and Account Resolution:**

1. `router/router.go` calls `stored_requests/config.NewStoredRequests` to build fetchers for auctions, AMP, video, accounts, categories, and stored responses.
2. `stored_requests/config/config.go` composes file, database, and HTTP fetchers plus optional in-memory caches and event listeners.
3. `account.GetAccount` in `account/account.go` merges fetched account JSON with `config.AccountDefaults` and returns a fully derived account object for request-time use.

**State Management:**
- Global immutable-ish state is held in long-lived structs such as `config.Configuration`, `router.Router`, the metrics engine from `metrics/config/metrics.go`, and the exchange built in `exchange/exchange.go`.
- Request-scoped mutable state is wrapped in `openrtb_ext.RequestWrapper` and `exchange.AuctionRequest`.
- Background state is refreshed by ticker tasks such as the currency fetch task in `main.go`, live GVL refresh task in `router/router.go`, and stored-request event polling in `stored_requests/config/config.go`.

## Key Abstractions

**Configuration Object:**
- Purpose: Single source of runtime settings and merged defaults.
- Examples: `config/config.go`, `account/account.go`
- Pattern: Populate once at startup, validate, derive helper maps, then inject into constructors.

**Router Composition Root:**
- Purpose: Central place where infra services, fetchers, hooks, adapters, and handlers are assembled.
- Examples: `router/router.go`, `router/admin.go`
- Pattern: Build dependencies eagerly, register routes explicitly, return a thin `Router` wrapper with shutdown hooks.

**Endpoint Dependency Bundle:**
- Purpose: Keep OpenRTB endpoint construction explicit without a global service locator.
- Examples: `endpoints/openrtb2/auction.go`, `endpoints/openrtb2/amp_auction.go`, `endpoints/openrtb2/video_auction.go`
- Pattern: `endpointDeps` stores collaborators and exposes handler methods such as `Auction`, `AmpAuction`, and `VideoAuctionEndpoint`.

**Auction Contract:**
- Purpose: Separate HTTP parsing from auction execution.
- Examples: `exchange/exchange.go`, `exchange/auction_response.go`
- Pattern: Endpoints pass an `exchange.AuctionRequest` into `exchange.Exchange.HoldAuction` and receive an `exchange.AuctionResponse`.

**Bidder Adapter Boundary:**
- Purpose: Encapsulate bidder-specific request/response translation and HTTP exchange.
- Examples: `exchange/bidder.go`, `adapters/33across/33across.go`, `adapters/appnexus/appnexus.go`
- Pattern: Every bidder implements `adapters.Bidder`; `exchange.AdaptBidder` wraps it into `exchange.AdaptedBidder` for auction-core use.

**Hook Planning and Execution:**
- Purpose: Apply host and account-configured modules at defined auction stages.
- Examples: `hooks/plan.go`, `hooks/repo.go`, `hooks/hookexecution/executor.go`
- Pattern: Build a `hooks.ExecutionPlanBuilder` once, then create a per-request hook executor that runs stage-specific hook groups with timeouts.

## Entry Points

**Application Entrypoint:**
- Location: `main.go`
- Triggers: Process start via `go run .`, `go build`, or container startup.
- Responsibilities: Parse flags, load bidder metadata and configuration, start currency refresh, create router, start servers.

**Router Construction:**
- Location: `router/router.go`
- Triggers: Called from `serve` in `main.go`.
- Responsibilities: Build HTTP clients, fetchers, metrics, analytics, modules, hooks, exchange, endpoint handlers, and route registrations.

**Main Server Listener:**
- Location: `server/server.go`
- Triggers: Called from `serve` in `main.go`.
- Responsibilities: Start main/admin/prometheus listeners, apply gzip and graceful shutdown, manage signal fan-out.

**Auction Endpoint Constructor:**
- Location: `endpoints/openrtb2/auction.go`
- Triggers: Called from `router/router.go`.
- Responsibilities: Validate required collaborators and produce the `POST /openrtb2/auction` handler.

**Video Endpoint Constructor:**
- Location: `endpoints/openrtb2/video_auction.go`
- Triggers: Called from `router/router.go`.
- Responsibilities: Produce the deprecated video handler that transforms simplified video requests into OpenRTB auctions.

## Error Handling

**Strategy:** Fail fast at startup, accumulate structured request-time warnings and errors during auction processing, and convert fatal request errors into HTTP responses.

**Patterns:**
- Startup and dependency-construction failures usually terminate the process with `logger.Fatalf`, as seen in `main.go`, `router/router.go`, `config/config.go`, and `server/prometheus.go`.
- Request parsing and validation accumulate `[]error` using the `errortypes` package in `endpoints/openrtb2/auction.go`; fatal errors are written immediately, while warning-only errors are carried into `response.ext`.
- Auction execution returns both errors and bidder-scoped messages; `exchange/exchange.go` appends warnings into `openrtb_ext.ExtBidResponse` before the final response is marshaled.

## Cross-Cutting Concerns

**Logging:** Global package-level logging through `logger` is used across `main.go`, `router/router.go`, `server/*`, `exchange/*`, `config/*`, and `stored_requests/config/config.go`.

**Validation:** Request validation is enforced by `ortb.NewRequestValidator` in `router/router.go`, bidder parameter validation is built by `openrtb_ext.NewBidderParamsValidator`, and config validation lives in `config/config.go`.

**Authentication:** No general request authentication middleware is registered in `router/router.go`. Request access control is configuration-driven through account lookup in `account/account.go`, privacy enforcement in `privacy/` and `gdpr/`, and user-sync protections in `pbs.UserSyncDeps` wired from `router/router.go`.

---

*Architecture analysis: 2026-04-05*
