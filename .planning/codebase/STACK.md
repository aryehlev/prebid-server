# Technology Stack

**Analysis Date:** 2026-04-05

## Languages

**Primary:**
- Go 1.23 minimum module target, validated in CI on Go 1.23.x and 1.24.x, and built in container/devcontainer with Go 1.24. Used for the server, adapters, modules, storage, and metrics in `go.mod`, `main.go`, `Dockerfile`, `.devcontainer/devcontainer.json`, and `.github/workflows/validate.yml`

**Secondary:**
- YAML - host configuration, bidder metadata, and samples in `docs/developers/configuration.md`, `static/bidder-info/*.yaml`, and `sample/001_banner/app.yaml`
- JSON - sample requests, schemas, and config payloads in `modules/prebid/rulesengine/config/rules-engine-schema.json`, `router/bidder_params_tests/appnexus.json`, and `sample/001_banner/stored_request.json`
- Bash - validation and formatting scripts in `validate.sh`, `scripts/format.sh`, `scripts/check_coverage.sh`, and `scripts/coverage.sh`
- JavaScript - GitHub workflow helpers in `.github/workflows/helpers/pull-request-utils.js` and `.github/workflows/scripts/send-notification-on-change.js`

## Runtime

**Environment:**
- Long-running Go HTTP service started from `main.go`
- Main HTTP listener uses `host` + `port` and defaults to `:8000`; admin listener uses `admin_port` and defaults to `:6060`, configured in `config/config.go` and served by `server/server.go`
- Optional Unix socket listener is supported through `unix_socket_enable` and `unix_socket_name` in `config/config.go` and `server/server.go`
- Docker runtime targets Ubuntu 22.04 in `Dockerfile`

**Package Manager:**
- Go modules via `go.mod`
- Lockfile: present in `go.sum`

## Frameworks

**Core:**
- Go stdlib `net/http` - base server/client runtime in `main.go`, `router/router.go`, `server/server.go`, and `prebid_cache_client/client.go`
- `github.com/julienschmidt/httprouter` v1.3.0 - request routing in `router/router.go`
- `github.com/spf13/viper` v1.12.0 - config loading and env binding in `config/config.go`
- `github.com/prebid/openrtb/v20` v20.3.0 - OpenRTB request/response model layer declared in `go.mod` and used across `router/router.go` and `endpoints/openrtb2/*`

**Testing:**
- Go `testing` package and `go test` orchestration in `validate.sh`
- `github.com/stretchr/testify` v1.8.1 - assertions and helpers declared in `go.mod`
- `github.com/DATA-DOG/go-sqlmock` v1.5.0 - DB mocking declared in `go.mod`

**Build/Dev:**
- `make` targets for deps, test, build, module generation, image build, and formatting in `Makefile`
- `go generate` for module registration in `Makefile` and `modules/modules.go`
- Docker multi-stage build in `Dockerfile`
- VS Code dev container in `.devcontainer/devcontainer.json` and `.devcontainer/Dockerfile`
- GitHub Actions validation in `.github/workflows/validate.yml`

## Key Dependencies

**Critical:**
- `github.com/prebid/openrtb/v20` v20.3.0 - canonical OpenRTB types and serialization declared in `go.mod`
- `github.com/spf13/viper` v1.12.0 - central configuration system in `config/config.go`
- `github.com/json-iterator/go` v1.1.12 - custom JSON handling initialized in `main.go`
- `github.com/rs/cors` v1.11.0 - permissive credentialed CORS wrapper in `router/router.go`
- `github.com/prometheus/client_golang` v1.12.1 - Prometheus exporter wired by `metrics/config/metrics.go` and `server/prometheus.go`
- `github.com/vrischmann/go-metrics-influxdb` v0.1.1 - InfluxDB metrics sink in `metrics/config/metrics.go`

**Infrastructure:**
- `github.com/go-sql-driver/mysql` v1.6.0 - optional stored-request database backend, imported in `router/router.go`
- `github.com/lib/pq` v1.10.4 - optional PostgreSQL stored-request database backend, imported in `router/router.go`
- `github.com/coocood/freecache` v1.2.1 - in-memory cache for stored requests and price floors in `stored_requests/caches/memory/cache.go` and `floors/fetcher.go`
- `github.com/51Degrees/device-detection-go/v4` v4.4.35 - on-prem device detection module in `modules/fiftyonedegrees/devicedetection/module.go`
- `github.com/IABTechLab/adscert` v0.34.0 - ads.cert signing support in `experiment/adscert/signer.go`
- `google.golang.org/grpc` v1.56.3 - remote ads.cert signer transport in `experiment/adscert/remotesigner.go`
- `github.com/prebid/go-gdpr` v1.12.0 and `github.com/prebid/go-gpp` v0.2.0 - privacy framework dependencies declared in `go.mod` and used from `router/router.go`

## Configuration

**Environment:**
- Config precedence is environment variables, then `pbs.json`, then `pbs.yaml`, with files read from the application directory or `/etc/config`, as documented in `docs/developers/configuration.md` and implemented in `config/config.go`
- Environment variables use the `PBS_` prefix and replace `.` with `_`, implemented by `config.SetupViper()` in `config/config.go`
- Concrete examples include `PBS_GDPR_DEFAULT_VALUE`, `PBS_EXTERNAL_URL`, and `PBS_PORT`, documented in `docs/developers/configuration.md`
- Bidder metadata is loaded at startup from `./static/bidder-info` in `main.go`; the repo currently contains 348 bidder info YAML files under `static/bidder-info/`
- No `.env` or `.env.*` files were detected during this pass under the repo root and its immediate children
- Secrets are expected in env vars or config files, and startup logging redacts passwords and secrets according to `docs/developers/configuration.md`

**Build:**
- Build and validation entry points are `Makefile` and `validate.sh`
- Container build config is `Dockerfile`
- Local containerized development config is `.devcontainer/devcontainer.json` and `.devcontainer/Dockerfile`
- CI config is `.github/workflows/validate.yml`
- Cross-platform build notes are maintained in `docs/build/README.md`

## Platform Requirements

**Development:**
- Go 1.23 or newer is required according to `README.md`
- Bash is required for helper scripts such as `validate.sh` and `scripts/format.sh`
- `cgo` must stay enabled because some modules compile native code, documented in `README.md`, `docs/build/README.md`, and enforced in `Dockerfile`
- A C compiler such as `gcc` is required for builds using native code modules, documented in `README.md` and `docs/build/README.md`

**Production:**
- Runtime container is Ubuntu 22.04 in `Dockerfile`
- Runtime dependencies include `ca-certificates`, `mtr`, and `libatomic1` in `Dockerfile`
- The service expects `static/` to be present at runtime, documented in `README.md` and copied in `Dockerfile`
- File-backed stored request data can also be shipped with the image via `stored_requests/data`, as shown in `Dockerfile` and `sample/docker-compose.yml`

---

*Stack analysis: 2026-04-05*
