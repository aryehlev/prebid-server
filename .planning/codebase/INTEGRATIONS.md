# External Integrations

**Analysis Date:** 2026-04-05

## APIs & External Services

**Demand Partners / RTB Bidders:**
- Prebid Server fans auction traffic out to adapter-defined bidder endpoints using the shared HTTP client from `router/router.go`, with bidder metadata loaded from `static/bidder-info/*.yaml` by `config.LoadBidderInfoFromDisk()` in `main.go`
  - SDK/Client: Go stdlib `net/http`; bidder config model in `config/bidderinfo.go`
  - Auth: per-bidder config and env bindings such as `PBS_ADAPTERS_<BIDDER>_ENDPOINT`, `PBS_ADAPTERS_<BIDDER>_XAPI_USERNAME`, `PBS_ADAPTERS_<BIDDER>_XAPI_PASSWORD`, `PBS_ADAPTERS_<BIDDER>_APP_SECRET`, `PBS_ADAPTERS_<BIDDER>_PLATFORM_ID`, and `PBS_ADAPTERS_<BIDDER>_USERSYNC_*` in `config/config.go`
- The repo currently contains 260 adapter packages under `adapters/` and 348 bidder metadata files under `static/bidder-info/`

**Prebid Cache:**
- Auction artifacts and video tracking payloads are stored in an external Prebid Cache service from `prebid_cache_client/client.go`; the client is constructed in `router/router.go`
  - SDK/Client: Go stdlib `net/http`
  - Auth: no auth fields are defined in core cache config; host and scheme are configured under `cache.*` and `external_cache.*` in `config/config.go`

**Currency Rates Feed:**
- Currency conversion rates are fetched on a ticker from the configured `currency_converter.fetch_url` in `main.go` and `currency/rate_converter.go`
  - SDK/Client: Go stdlib `net/http`
  - Auth: none; URL configured via `currency_converter.fetch_url` or `PBS_CURRENCY_CONVERTER_FETCH_URL` in `config/config.go`

**GDPR Global Vendor List:**
- The IAB vendor list is fetched from `https://vendor-list.consensu.org/...` in `gdpr/vendorlist-fetching.go`
  - SDK/Client: `github.com/prebid/go-gdpr`
  - Auth: none; refresh cadence configured with `gdpr.live_gvl_refresh_interval_seconds` in `config/config.go`

**Analytics Providers:**
- AGMA analytics buffers and POSTs analytics events from `analytics/agma/agma_module.go`, with defaults in `config/config.go` and usage docs in `analytics/agma/README.md`
  - SDK/Client: Go stdlib `net/http`
  - Auth: account codes and publisher mappings under `analytics.agma.accounts`; endpoint config under `analytics.agma.endpoint.*`
- Pubstack analytics sends auction, AMP, cookie sync, setuid, and video telemetry from `analytics/pubstack/pubstack_module.go`, with docs in `analytics/pubstack/README.md`
  - SDK/Client: Go stdlib `net/http`
  - Auth: `analytics.pubstack.scopeid` or `PBS_ANALYTICS_PUBSTACK_SCOPEID`; endpoint under `analytics.pubstack.endpoint`

**Privacy / Opt-Out Verification:**
- Google reCAPTCHA is used to protect the `/optout` flow in `pbs/usersync.go`
  - SDK/Client: Go stdlib `net/http`
  - Auth: `recaptcha_secret` or `PBS_RECAPTCHA_SECRET` in `config/config.go`

**Ads.cert Signing:**
- Experimental ads.cert request signing supports both in-process signing and a remote gRPC signatory in `experiment/adscert/signer.go`, `experiment/adscert/remotesigner.go`, and `config/experiment.go`
  - SDK/Client: `github.com/IABTechLab/adscert` and `google.golang.org/grpc`
  - Auth: `experiment.adscert.inprocess.key` for in-process signing, or `experiment.adscert.remote.url` for the remote signer in `config/experiment.go`

**Hook Modules / Enrichment Providers:**
- Scope3 RTD integrates Scope3’s real-time data API, documented in `modules/scope3/rtd/README.md`
  - SDK/Client: shared HTTP client passed through the hooks/module system in `router/router.go`
  - Auth: `hooks.modules.scope3.rtd.auth_key` in host config; the module README shows `${SCOPE3_API_KEY}` as the source value in `modules/scope3/rtd/README.md`
- 51Degrees device detection operates on-prem but can auto-download updated data files as documented in `modules/fiftyonedegrees/devicedetection/README.md`
  - SDK/Client: `github.com/51Degrees/device-detection-go/v4`
  - Auth: `hooks.modules.fiftyonedegrees.devicedetection.data_file.update.license_key`

**Stored Data / Remote Configuration Endpoints:**
- Stored requests, AMP requests, video requests, stored responses, account data, and category mappings can all be loaded via HTTP endpoints configured through `config/stored_requests.go` and wired in `stored_requests/config/config.go`
  - SDK/Client: Go stdlib `net/http`
  - Auth: no built-in auth fields detected; endpoints include `stored_requests.http.endpoint`, `accounts.http.endpoint`, and `category_mapping.http.endpoint`
- Account-level price floors can be pulled from remote URLs in `floors/fetcher.go` with defaults defined in `config/config.go`
  - SDK/Client: Go stdlib `net/http`
  - Auth: no built-in auth fields detected; URL configured in `account_defaults.price_floors.fetch.url`

## Data Storage

**Databases:**
- MySQL and PostgreSQL are optional backends for stored requests and related stored data in `stored_requests/config/config.go` and `config/stored_requests.go`
  - Connection: `stored_requests.database.connection.*`, `stored_video_req.database.connection.*`, and related section-specific keys in `config/config.go`
  - Client: Go `database/sql` with `github.com/go-sql-driver/mysql` and `github.com/lib/pq`, imported in `router/router.go`

**File Storage:**
- Local filesystem storage is first-class for static bidder metadata and sample/default stored data in `static/bidder-info/`, `stored_requests/data/by_id/`, and `stored_responses/data/by_id/`
- The sample deployment mounts config and stored data files through `sample/docker-compose.yml`
- The 51Degrees module also requires a local writable device data file path, documented in `modules/fiftyonedegrees/devicedetection/README.md`

**Caching:**
- In-process caching uses `github.com/coocood/freecache` in `stored_requests/caches/memory/cache.go` and `floors/fetcher.go`
- External Prebid Cache is used for cacheable auction payloads in `prebid_cache_client/client.go`

## Authentication & Identity

**Auth Provider:**
- No first-party user auth provider, OAuth server, or SSO integration is detected for the main service
  - Implementation: identity and match-state handling are cookie/user-sync based through `/cookie_sync`, `/setuid`, and `/getuids` in `router/router.go`; privacy enforcement uses GDPR/GPP libraries and account controls assembled in `router/router.go`

## Monitoring & Observability

**Error Tracking:**
- None detected

**Logs:**
- Application logging goes through the internal `logger` package described in `README.md`
- Optional file-based analytics logging can be enabled through `analytics.file.filename` in `config/config.go` and `analytics/build/build.go`
- Prometheus metrics are exposed from `server/prometheus.go`
- InfluxDB metrics are pushed from `metrics/config/metrics.go`

## CI/CD & Deployment

**Hosting:**
- Containerized deployment is the primary path in `Dockerfile` and `README.md`
- Kubernetes/config-map deployment is explicitly documented in `README.md`

**CI Pipeline:**
- Validation pipeline runs in GitHub Actions from `.github/workflows/validate.yml`
- Security and static analysis workflows exist in `.github/workflows/security.yml` and `.github/workflows/semgrep.yml`
- Release/publish workflows exist in `.github/workflows/release.yml` and `.github/workflows/publishonly.yml`

## Environment Configuration

**Required env vars:**
- `PBS_GDPR_DEFAULT_VALUE` is required for startup according to `README.md`
- Core host/runtime vars include `PBS_EXTERNAL_URL`, `PBS_PORT`, `PBS_ADMIN_PORT`, and `PBS_HOST`, as documented in `docs/developers/configuration.md`
- Common integration vars include `PBS_RECAPTCHA_SECRET`, `PBS_CURRENCY_CONVERTER_FETCH_URL`, `PBS_METRICS_PROMETHEUS_PORT`, `PBS_METRICS_INFLUXDB_HOST`, and `PBS_ANALYTICS_PUBSTACK_*`
- Bidder-specific integration vars follow the `PBS_ADAPTERS_<BIDDER>_*` pattern in `config/config.go`

**Secrets location:**
- Secrets are expected in environment variables or in `pbs.json` / `pbs.yaml`, with config loaded by `config.SetupViper()` in `config/config.go`
- Startup config logging redacts passwords and secrets according to `docs/developers/configuration.md`
- No secret files were read during this pass, and no `.env` files were detected under the repo root

## Webhooks & Callbacks

**Incoming:**
- Public HTTP endpoints are registered in `router/router.go`: `/openrtb2/auction`, `/openrtb2/video`, `/openrtb2/amp`, `/cookie_sync`, `/setuid`, `/getuids`, `/event`, `/vtrack`, `/optout`, `/status`, `/info/bidders`, `/version`, and `/bidders/params`
- Configurable stored-data cache event endpoints are added by `stored_requests/config/config.go` when `cache_events.enabled` is set
- A Prometheus scrape endpoint is served on a dedicated listener when `metrics.prometheus.port` is configured in `server/prometheus.go`

**Outgoing:**
- Bidder user-sync redirects and callbacks are templated in `config/bidderinfo.go` and route back through `/setuid`
- Event tracking URLs are generated from `account_defaults.events.default_url` in `config/config.go`
- The service makes outbound HTTP or gRPC calls to bidder endpoints, Prebid Cache, currency feeds, the IAB vendor list service, analytics providers, reCAPTCHA, stored-data HTTP backends, price floor sources, Scope3, 51Degrees update URLs, and the remote ads.cert signer from `router/router.go`, `main.go`, `pbs/usersync.go`, `stored_requests/config/config.go`, and `experiment/adscert/remotesigner.go`

---

*Integration audit: 2026-04-05*
