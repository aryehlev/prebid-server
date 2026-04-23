<!-- GSD:project-start source:PROJECT.md -->
## Project

**Prebid Server Rust Parity Port**

This project is to finish porting the existing Go Prebid Server codebase to Rust under `rust/`. The goal is a Rust implementation that can replace the Go server for production traffic while preserving the current server's business behavior, configuration shape, and operational expectations. Longer term, this creates an open source Rust Prebid Server base that can later carry client-side code.

**Core Value:** The Rust server can replace the Go server for production traffic without changing application behavior.

### Constraints

- **Behavior**: Rust behavior must match the Go server exactly, even when the Go design is awkward — parity is the objective
- **Compatibility**: Configuration and operational behavior must remain compatible with the existing Go server — replacement depends on it
- **Scope**: Work should focus on app code and business features first — non-functional tuning happens later
- **Reference Implementation**: The Go codebase is the source of truth for expected behavior during the port — parity decisions resolve against it
<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->
## Technology Stack

## Languages
- Go 1.23 minimum module target, validated in CI on Go 1.23.x and 1.24.x, and built in container/devcontainer with Go 1.24. Used for the server, adapters, modules, storage, and metrics in `go.mod`, `main.go`, `Dockerfile`, `.devcontainer/devcontainer.json`, and `.github/workflows/validate.yml`
- YAML - host configuration, bidder metadata, and samples in `docs/developers/configuration.md`, `static/bidder-info/*.yaml`, and `sample/001_banner/app.yaml`
- JSON - sample requests, schemas, and config payloads in `modules/prebid/rulesengine/config/rules-engine-schema.json`, `router/bidder_params_tests/appnexus.json`, and `sample/001_banner/stored_request.json`
- Bash - validation and formatting scripts in `validate.sh`, `scripts/format.sh`, `scripts/check_coverage.sh`, and `scripts/coverage.sh`
- JavaScript - GitHub workflow helpers in `.github/workflows/helpers/pull-request-utils.js` and `.github/workflows/scripts/send-notification-on-change.js`
## Runtime
- Long-running Go HTTP service started from `main.go`
- Main HTTP listener uses `host` + `port` and defaults to `:8000`; admin listener uses `admin_port` and defaults to `:6060`, configured in `config/config.go` and served by `server/server.go`
- Optional Unix socket listener is supported through `unix_socket_enable` and `unix_socket_name` in `config/config.go` and `server/server.go`
- Docker runtime targets Ubuntu 22.04 in `Dockerfile`
- Go modules via `go.mod`
- Lockfile: present in `go.sum`
## Frameworks
- Go stdlib `net/http` - base server/client runtime in `main.go`, `router/router.go`, `server/server.go`, and `prebid_cache_client/client.go`
- `github.com/julienschmidt/httprouter` v1.3.0 - request routing in `router/router.go`
- `github.com/spf13/viper` v1.12.0 - config loading and env binding in `config/config.go`
- `github.com/prebid/openrtb/v20` v20.3.0 - OpenRTB request/response model layer declared in `go.mod` and used across `router/router.go` and `endpoints/openrtb2/*`
- Go `testing` package and `go test` orchestration in `validate.sh`
- `github.com/stretchr/testify` v1.8.1 - assertions and helpers declared in `go.mod`
- `github.com/DATA-DOG/go-sqlmock` v1.5.0 - DB mocking declared in `go.mod`
- `make` targets for deps, test, build, module generation, image build, and formatting in `Makefile`
- `go generate` for module registration in `Makefile` and `modules/modules.go`
- Docker multi-stage build in `Dockerfile`
- VS Code dev container in `.devcontainer/devcontainer.json` and `.devcontainer/Dockerfile`
- GitHub Actions validation in `.github/workflows/validate.yml`
## Key Dependencies
- `github.com/prebid/openrtb/v20` v20.3.0 - canonical OpenRTB types and serialization declared in `go.mod`
- `github.com/spf13/viper` v1.12.0 - central configuration system in `config/config.go`
- `github.com/json-iterator/go` v1.1.12 - custom JSON handling initialized in `main.go`
- `github.com/rs/cors` v1.11.0 - permissive credentialed CORS wrapper in `router/router.go`
- `github.com/prometheus/client_golang` v1.12.1 - Prometheus exporter wired by `metrics/config/metrics.go` and `server/prometheus.go`
- `github.com/vrischmann/go-metrics-influxdb` v0.1.1 - InfluxDB metrics sink in `metrics/config/metrics.go`
- `github.com/go-sql-driver/mysql` v1.6.0 - optional stored-request database backend, imported in `router/router.go`
- `github.com/lib/pq` v1.10.4 - optional PostgreSQL stored-request database backend, imported in `router/router.go`
- `github.com/coocood/freecache` v1.2.1 - in-memory cache for stored requests and price floors in `stored_requests/caches/memory/cache.go` and `floors/fetcher.go`
- `github.com/51Degrees/device-detection-go/v4` v4.4.35 - on-prem device detection module in `modules/fiftyonedegrees/devicedetection/module.go`
- `github.com/IABTechLab/adscert` v0.34.0 - ads.cert signing support in `experiment/adscert/signer.go`
- `google.golang.org/grpc` v1.56.3 - remote ads.cert signer transport in `experiment/adscert/remotesigner.go`
- `github.com/prebid/go-gdpr` v1.12.0 and `github.com/prebid/go-gpp` v0.2.0 - privacy framework dependencies declared in `go.mod` and used from `router/router.go`
## Configuration
- Config precedence is environment variables, then `pbs.json`, then `pbs.yaml`, with files read from the application directory or `/etc/config`, as documented in `docs/developers/configuration.md` and implemented in `config/config.go`
- Environment variables use the `PBS_` prefix and replace `.` with `_`, implemented by `config.SetupViper()` in `config/config.go`
- Concrete examples include `PBS_GDPR_DEFAULT_VALUE`, `PBS_EXTERNAL_URL`, and `PBS_PORT`, documented in `docs/developers/configuration.md`
- Bidder metadata is loaded at startup from `./static/bidder-info` in `main.go`; the repo currently contains 348 bidder info YAML files under `static/bidder-info/`
- No `.env` or `.env.*` files were detected during this pass under the repo root and its immediate children
- Secrets are expected in env vars or config files, and startup logging redacts passwords and secrets according to `docs/developers/configuration.md`
- Build and validation entry points are `Makefile` and `validate.sh`
- Container build config is `Dockerfile`
- Local containerized development config is `.devcontainer/devcontainer.json` and `.devcontainer/Dockerfile`
- CI config is `.github/workflows/validate.yml`
- Cross-platform build notes are maintained in `docs/build/README.md`
## Platform Requirements
- Go 1.23 or newer is required according to `README.md`
- Bash is required for helper scripts such as `validate.sh` and `scripts/format.sh`
- `cgo` must stay enabled because some modules compile native code, documented in `README.md`, `docs/build/README.md`, and enforced in `Dockerfile`
- A C compiler such as `gcc` is required for builds using native code modules, documented in `README.md` and `docs/build/README.md`
- Runtime container is Ubuntu 22.04 in `Dockerfile`
- Runtime dependencies include `ca-certificates`, `mtr`, and `libatomic1` in `Dockerfile`
- The service expects `static/` to be present at runtime, documented in `README.md` and copied in `Dockerfile`
- File-backed stored request data can also be shipped with the image via `stored_requests/data`, as shown in `Dockerfile` and `sample/docker-compose.yml`
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

## Naming Patterns
- Use lowercase snake_case file names for multiword files: `config/requestvalidation.go`, `exchange/bidder_validate_bids.go`, `stored_requests/backends/db_provider/mysql_dbprovider_test.go`.
- Keep package names lowercase and directory-driven: `config`, `openrtb2`, `jsonutil`, `db_provider`, `hookexecution`.
- Reserve `_test.go` for tests and `doc.go` for package-level docs when needed: `util/iterutil/doc.go`, `endpoints/openrtb2/auction_test.go`.
- Exported functions and methods use Go CamelCase: `config.SetupViper`, `logger.Infof`, `openrtb2.NewEndpoint`.
- Unexported helpers use lowerCamelCase: `runJsonBasedTest` in `endpoints/openrtb2/auction_test.go`, `fakeQueryRegex` in `stored_requests/events/database/database_test.go`.
- Test names follow Go defaults: `Test...`, `Benchmark...`, and race-only tests use `TestRace...` so `validate.sh` can target them with `go test -race -run ^TestRace.*$`.
- Local variables stay short and contextual in narrow scopes: `cfg`, `errs`, `tt`, `tc`, `req`, `resp`, `mock`.
- Table-driven tests usually use `tests`, `testCases`, `tt`, or `tc`: `config/config_test.go`, `stored_requests/backends/db_provider/db_provider_test.go`, `analytics/agma/agma_module_test.go`.
- Package-level test fixtures use descriptive globals when reused across many cases: `bidderInfos` in `config/config_test.go`, `mockValidAuctionObject` in `analytics/agma/agma_module_test.go`.
- Exported structs and interfaces use PascalCase: `Configuration` in `config/config.go`, `Logger` in `logger/interface.go`, `BidderInfo` in `config/bidderinfo.go`.
- Test-only mock and fake types are prefixed with `mock`, `Mock`, or `Fake`: `MockLogger` in `analytics/filesystem/file_module_test.go`, `mockLogger` in `logger/logger_test.go`, `FakeTime` in `stored_requests/events/database/database_test.go`.
## Code Style
- Use `gofmt -s` as the canonical formatter. `scripts/format.sh` runs `gofmt -s -l` and optionally rewrites files with `gofmt -s -w`.
- Use `./validate.sh` or `make test` before concluding changes; both run formatting checks first.
- Keep imports in Go default format: standard library first, then one non-stdlib block for all external and module imports. `config/config.go` and `endpoints/openrtb2/auction_test.go` show the repo-standard layout.
- No `golangci-lint`, ESLint, Biome, or Prettier config is detected in the repo root.
- The enforced quality gates are `gofmt -s`, `go test`, targeted race tests, and `go vet` from `validate.sh`.
- Existing inline suppressions use standard Go lint comments when needed, for example `//nolint: errcheck` in `server/server_test.go`.
## Import Organization
- Use aliases only to resolve collisions or improve readability: `jsoniter` in `endpoints/openrtb2/auction_test.go`, `analyticsBuild` in `endpoints/openrtb2/auction_benchmark_test.go`, `metricsConfig` in `endpoints/openrtb2/auction_test.go`.
- Prefer full package names when no alias is needed: `github.com/stretchr/testify/assert`, `github.com/prebid/prebid-server/v4/util/jsonutil`.
## Error Handling
- Return early on errors and keep happy paths left-aligned:
- Add context with `fmt.Errorf` for user-facing or boundary errors. Use `%w` when preserving wrapped errors matters, as in `analytics/pubstack/config.go` and `injector/injector.go`.
- Accumulate validation failures in `[]error` rather than failing on the first issue inside config validation paths: `config/config.go`, `config/stored_requests.go`, `config/account.go`.
- Use typed or aggregated error helpers for domain errors instead of raw strings where a subsystem already exposes one: `errortypes.NewAggregateError` in `router/router.go`.
- Use `logger.Fatalf` only in process bootstrap or unrecoverable startup paths such as `router/router.go` and `analytics/build/build.go`; regular request and module paths return errors instead.
## Logging
- Log through `logger.Debugf`, `logger.Infof`, `logger.Warnf`, `logger.Errorf`, and `logger.Fatalf` from `logger/logger.go`.
- Prefer formatted messages with subsystem prefixes for operational logs: `[pubstack]` in `analytics/pubstack/pubstack_module.go`, `[AgmaAnalytics]` in `analytics/agma/agma_module.go`, `[PBS Router]` in `router/router.go`.
- Keep logs at boundaries and failures, not inside trivial data transforms.
- Tests that validate logging behavior replace the package-global logger with a mock implementation instead of intercepting stdout: `logger/logger_test.go`.
## Comments
- Add comments for exported types/functions and non-obvious implementation constraints. `config/config.go` documents config fields with operational meaning, and `endpoints/openrtb2/auction_benchmark_test.go` explains benchmark fixtures.
- Use comments to explain why a workaround exists, not to narrate obvious assignments. `scripts/coverage.sh` and `util/jsonutil/merge_test.go` are representative.
- Prefer package docs in `doc.go` for reusable utility packages. `util/iterutil/doc.go` is the clearest package-level example.
- Not applicable; this repository is Go, and uses Go doc comments instead.
## Function Design
- Pass config and mutable domain state as pointers when mutation or large structs are involved: `func (cfg *Configuration) validate(...)` in `config/config.go`.
- Prefer explicit context objects over long primitive parameter lists in newer orchestration code: hook handlers in `hooks/hookexecution/mocks_test.go` and request handlers under `endpoints/openrtb2/`.
- Multi-value returns follow Go norms: result plus `error`, or multiple domain values plus `error`/`[]error`.
- Validation and fetch code frequently returns collections plus error lists instead of collapsing everything into one error: `stored_requests/backends/db_fetcher/fetcher.go`, `config/config.go`.
## Module Design
- Keep package APIs explicit from concrete files; exported identifiers live in the package file where behavior is implemented rather than behind generated interfaces.
- Package-level facades are used when the package wants a narrow entry surface, for example `logger/logger.go`.
- Not used. There are no JS-style index barrels; package boundaries are standard Go package directories such as `exchange/`, `privacy/`, `stored_requests/`, and `util/jsonutil/`.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

## Pattern Overview
- Startup is centralized in `main.go`, which loads bidder metadata from `static/bidder-info/`, builds a `config.Configuration`, creates the router, and starts the HTTP servers.
- Dependency wiring is concentrated in `router/router.go`; handlers and services receive prebuilt collaborators instead of constructing them lazily.
- Auction execution is split between HTTP-facing endpoint code in `endpoints/openrtb2/*.go` and cross-bidder orchestration in `exchange/exchange.go`.
## Layers
- Purpose: Start the process, load static and dynamic configuration, create background tasks, and own server lifecycle.
- Location: `main.go`, `server/server.go`, `server/listener.go`, `server/prometheus.go`
- Contains: Process entrypoint, config loading, server listeners, graceful shutdown, compression, metrics listener setup.
- Depends on: `config`, `router`, `currency`, `util/task`, `metrics/config`
- Used by: The whole application runtime.
- Purpose: Define host configuration, account defaults, bidder metadata, and runtime feature flags.
- Location: `config/config.go`, `config/bidderinfo.go`, `static/bidder-info/`, `static/bidder-params/`, `static/category-mapping/`
- Contains: `config.Configuration`, bidder YAML parsing, Viper defaults, request validation settings, static bidder parameter schemas.
- Depends on: `viper`, `openrtb_ext`, `logger`, `util/jsonutil`
- Used by: `main.go`, `router/router.go`, `exchange/exchange.go`, `account/account.go`, `usersync`, `stored_requests`
- Purpose: Translate HTTP requests into internal request objects and marshal responses back to HTTP.
- Location: `router/router.go`, `router/admin.go`, `router/aspects/request_timeout_handler.go`, `endpoints/`, `endpoints/openrtb2/`
- Contains: Route registration, CORS wrapping, admin mux, request-size enforcement, endpoint-specific parsing, response writing.
- Depends on: `config`, `exchange`, `analytics`, `metrics`, `stored_requests`, `hooks/hookexecution`, `privacy`, `usersync`
- Used by: External clients calling `/openrtb2/auction`, `/openrtb2/amp`, `/openrtb2/video`, `/cookie_sync`, `/setuid`, `/event`, `/vtrack`, `/info/*`
- Purpose: Execute one auction across all relevant bidders and assemble the OpenRTB response.
- Location: `exchange/exchange.go`, `exchange/auction.go`, `exchange/bidder.go`, `exchange/auction_response.go`, `exchange/entities/`
- Contains: `exchange.Exchange`, `AuctionRequest`, request splitting, bidder fan-out, bid validation, price floors, category mapping, targeting, caching, seat non-bid handling.
- Depends on: `adapters`, `prebid_cache_client`, `currency`, `privacy`, `gdpr`, `floors`, `macros`, `metrics`, `stored_requests`, `stored_responses`
- Used by: `endpoints/openrtb2/auction.go`, `endpoints/openrtb2/amp_auction.go`, `endpoints/openrtb2/video_auction.go`
- Purpose: Plug in bidder-specific protocol logic and hook/module-based auction extensions.
- Location: `adapters/`, `exchange/adapter_builders.go`, `modules/modules.go`, `modules/builder.go`, `hooks/plan.go`, `hooks/repo.go`, `hooks/hookexecution/`
- Contains: One adapter package per bidder, generated adapter registration, module builders, hook repository, execution plans, per-stage hook execution.
- Depends on: `config`, `moduledeps`, `hookstage`, `exchange`, `openrtb_ext`
- Used by: `router/router.go` during startup and `exchange/*` during auction execution
- Purpose: Provide reusable subsystems around storage, privacy, analytics, metrics, and OpenRTB model extensions.
- Location: `stored_requests/`, `stored_responses/`, `analytics/build/build.go`, `metrics/config/metrics.go`, `privacy/`, `gdpr/`, `openrtb_ext/`, `account/account.go`
- Contains: Stored request fetchers and caches, analytics fan-out, metrics multiplexing, account resolution, privacy enforcement helpers, OpenRTB extension types.
- Depends on: External libraries and package-local helpers.
- Used by: `router/router.go`, `endpoints/openrtb2/*.go`, `exchange/*`, `endpoints/*`
## Data Flow
- Global immutable-ish state is held in long-lived structs such as `config.Configuration`, `router.Router`, the metrics engine from `metrics/config/metrics.go`, and the exchange built in `exchange/exchange.go`.
- Request-scoped mutable state is wrapped in `openrtb_ext.RequestWrapper` and `exchange.AuctionRequest`.
- Background state is refreshed by ticker tasks such as the currency fetch task in `main.go`, live GVL refresh task in `router/router.go`, and stored-request event polling in `stored_requests/config/config.go`.
## Key Abstractions
- Purpose: Single source of runtime settings and merged defaults.
- Examples: `config/config.go`, `account/account.go`
- Pattern: Populate once at startup, validate, derive helper maps, then inject into constructors.
- Purpose: Central place where infra services, fetchers, hooks, adapters, and handlers are assembled.
- Examples: `router/router.go`, `router/admin.go`
- Pattern: Build dependencies eagerly, register routes explicitly, return a thin `Router` wrapper with shutdown hooks.
- Purpose: Keep OpenRTB endpoint construction explicit without a global service locator.
- Examples: `endpoints/openrtb2/auction.go`, `endpoints/openrtb2/amp_auction.go`, `endpoints/openrtb2/video_auction.go`
- Pattern: `endpointDeps` stores collaborators and exposes handler methods such as `Auction`, `AmpAuction`, and `VideoAuctionEndpoint`.
- Purpose: Separate HTTP parsing from auction execution.
- Examples: `exchange/exchange.go`, `exchange/auction_response.go`
- Pattern: Endpoints pass an `exchange.AuctionRequest` into `exchange.Exchange.HoldAuction` and receive an `exchange.AuctionResponse`.
- Purpose: Encapsulate bidder-specific request/response translation and HTTP exchange.
- Examples: `exchange/bidder.go`, `adapters/33across/33across.go`, `adapters/appnexus/appnexus.go`
- Pattern: Every bidder implements `adapters.Bidder`; `exchange.AdaptBidder` wraps it into `exchange.AdaptedBidder` for auction-core use.
- Purpose: Apply host and account-configured modules at defined auction stages.
- Examples: `hooks/plan.go`, `hooks/repo.go`, `hooks/hookexecution/executor.go`
- Pattern: Build a `hooks.ExecutionPlanBuilder` once, then create a per-request hook executor that runs stage-specific hook groups with timeouts.
## Entry Points
- Location: `main.go`
- Triggers: Process start via `go run .`, `go build`, or container startup.
- Responsibilities: Parse flags, load bidder metadata and configuration, start currency refresh, create router, start servers.
- Location: `router/router.go`
- Triggers: Called from `serve` in `main.go`.
- Responsibilities: Build HTTP clients, fetchers, metrics, analytics, modules, hooks, exchange, endpoint handlers, and route registrations.
- Location: `server/server.go`
- Triggers: Called from `serve` in `main.go`.
- Responsibilities: Start main/admin/prometheus listeners, apply gzip and graceful shutdown, manage signal fan-out.
- Location: `endpoints/openrtb2/auction.go`
- Triggers: Called from `router/router.go`.
- Responsibilities: Validate required collaborators and produce the `POST /openrtb2/auction` handler.
- Location: `endpoints/openrtb2/video_auction.go`
- Triggers: Called from `router/router.go`.
- Responsibilities: Produce the deprecated video handler that transforms simplified video requests into OpenRTB auctions.
## Error Handling
- Startup and dependency-construction failures usually terminate the process with `logger.Fatalf`, as seen in `main.go`, `router/router.go`, `config/config.go`, and `server/prometheus.go`.
- Request parsing and validation accumulate `[]error` using the `errortypes` package in `endpoints/openrtb2/auction.go`; fatal errors are written immediately, while warning-only errors are carried into `response.ext`.
- Auction execution returns both errors and bidder-scoped messages; `exchange/exchange.go` appends warnings into `openrtb_ext.ExtBidResponse` before the final response is marshaled.
## Cross-Cutting Concerns
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, or `.github/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
