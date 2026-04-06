# Coding Conventions

**Analysis Date:** 2026-04-05

## Naming Patterns

**Files:**
- Use lowercase snake_case file names for multiword files: `config/requestvalidation.go`, `exchange/bidder_validate_bids.go`, `stored_requests/backends/db_provider/mysql_dbprovider_test.go`.
- Keep package names lowercase and directory-driven: `config`, `openrtb2`, `jsonutil`, `db_provider`, `hookexecution`.
- Reserve `_test.go` for tests and `doc.go` for package-level docs when needed: `util/iterutil/doc.go`, `endpoints/openrtb2/auction_test.go`.

**Functions:**
- Exported functions and methods use Go CamelCase: `config.SetupViper`, `logger.Infof`, `openrtb2.NewEndpoint`.
- Unexported helpers use lowerCamelCase: `runJsonBasedTest` in `endpoints/openrtb2/auction_test.go`, `fakeQueryRegex` in `stored_requests/events/database/database_test.go`.
- Test names follow Go defaults: `Test...`, `Benchmark...`, and race-only tests use `TestRace...` so `validate.sh` can target them with `go test -race -run ^TestRace.*$`.

**Variables:**
- Local variables stay short and contextual in narrow scopes: `cfg`, `errs`, `tt`, `tc`, `req`, `resp`, `mock`.
- Table-driven tests usually use `tests`, `testCases`, `tt`, or `tc`: `config/config_test.go`, `stored_requests/backends/db_provider/db_provider_test.go`, `analytics/agma/agma_module_test.go`.
- Package-level test fixtures use descriptive globals when reused across many cases: `bidderInfos` in `config/config_test.go`, `mockValidAuctionObject` in `analytics/agma/agma_module_test.go`.

**Types:**
- Exported structs and interfaces use PascalCase: `Configuration` in `config/config.go`, `Logger` in `logger/interface.go`, `BidderInfo` in `config/bidderinfo.go`.
- Test-only mock and fake types are prefixed with `mock`, `Mock`, or `Fake`: `MockLogger` in `analytics/filesystem/file_module_test.go`, `mockLogger` in `logger/logger_test.go`, `FakeTime` in `stored_requests/events/database/database_test.go`.

## Code Style

**Formatting:**
- Use `gofmt -s` as the canonical formatter. `scripts/format.sh` runs `gofmt -s -l` and optionally rewrites files with `gofmt -s -w`.
- Use `./validate.sh` or `make test` before concluding changes; both run formatting checks first.
- Keep imports in Go default format: standard library first, then one non-stdlib block for all external and module imports. `config/config.go` and `endpoints/openrtb2/auction_test.go` show the repo-standard layout.

**Linting:**
- No `golangci-lint`, ESLint, Biome, or Prettier config is detected in the repo root.
- The enforced quality gates are `gofmt -s`, `go test`, targeted race tests, and `go vet` from `validate.sh`.
- Existing inline suppressions use standard Go lint comments when needed, for example `//nolint: errcheck` in `server/server_test.go`.

## Import Organization

**Order:**
1. Standard library imports.
2. Third-party imports.
3. This module's imports, typically kept in the same non-stdlib block as third-party packages.

**Path Aliases:**
- Use aliases only to resolve collisions or improve readability: `jsoniter` in `endpoints/openrtb2/auction_test.go`, `analyticsBuild` in `endpoints/openrtb2/auction_benchmark_test.go`, `metricsConfig` in `endpoints/openrtb2/auction_test.go`.
- Prefer full package names when no alias is needed: `github.com/stretchr/testify/assert`, `github.com/prebid/prebid-server/v4/util/jsonutil`.

## Error Handling

**Patterns:**
- Return early on errors and keep happy paths left-aligned:

```go
if err != nil {
	return nil, err
}
```

Pattern source: `analytics/pubstack/pubstack_module.go`, `analytics/agma/sender.go`, `router/router.go`.

- Add context with `fmt.Errorf` for user-facing or boundary errors. Use `%w` when preserving wrapped errors matters, as in `analytics/pubstack/config.go` and `injector/injector.go`.
- Accumulate validation failures in `[]error` rather than failing on the first issue inside config validation paths: `config/config.go`, `config/stored_requests.go`, `config/account.go`.
- Use typed or aggregated error helpers for domain errors instead of raw strings where a subsystem already exposes one: `errortypes.NewAggregateError` in `router/router.go`.
- Use `logger.Fatalf` only in process bootstrap or unrecoverable startup paths such as `router/router.go` and `analytics/build/build.go`; regular request and module paths return errors instead.

## Logging

**Framework:** `logger` package wrapper over glog

**Patterns:**
- Log through `logger.Debugf`, `logger.Infof`, `logger.Warnf`, `logger.Errorf`, and `logger.Fatalf` from `logger/logger.go`.
- Prefer formatted messages with subsystem prefixes for operational logs: `[pubstack]` in `analytics/pubstack/pubstack_module.go`, `[AgmaAnalytics]` in `analytics/agma/agma_module.go`, `[PBS Router]` in `router/router.go`.
- Keep logs at boundaries and failures, not inside trivial data transforms.
- Tests that validate logging behavior replace the package-global logger with a mock implementation instead of intercepting stdout: `logger/logger_test.go`.

## Comments

**When to Comment:**
- Add comments for exported types/functions and non-obvious implementation constraints. `config/config.go` documents config fields with operational meaning, and `endpoints/openrtb2/auction_benchmark_test.go` explains benchmark fixtures.
- Use comments to explain why a workaround exists, not to narrate obvious assignments. `scripts/coverage.sh` and `util/jsonutil/merge_test.go` are representative.
- Prefer package docs in `doc.go` for reusable utility packages. `util/iterutil/doc.go` is the clearest package-level example.

**JSDoc/TSDoc:**
- Not applicable; this repository is Go, and uses Go doc comments instead.

## Function Design

**Size:** Use small wrappers for leaf utilities and larger orchestrator functions for endpoint/bootstrap assembly. `logger/logger.go` is the small-wrapper end; `router/router.go` and `endpoints/openrtb2/auction.go` are orchestration-heavy.

**Parameters:**
- Pass config and mutable domain state as pointers when mutation or large structs are involved: `func (cfg *Configuration) validate(...)` in `config/config.go`.
- Prefer explicit context objects over long primitive parameter lists in newer orchestration code: hook handlers in `hooks/hookexecution/mocks_test.go` and request handlers under `endpoints/openrtb2/`.

**Return Values:**
- Multi-value returns follow Go norms: result plus `error`, or multiple domain values plus `error`/`[]error`.
- Validation and fetch code frequently returns collections plus error lists instead of collapsing everything into one error: `stored_requests/backends/db_fetcher/fetcher.go`, `config/config.go`.

## Module Design

**Exports:**
- Keep package APIs explicit from concrete files; exported identifiers live in the package file where behavior is implemented rather than behind generated interfaces.
- Package-level facades are used when the package wants a narrow entry surface, for example `logger/logger.go`.

**Barrel Files:**
- Not used. There are no JS-style index barrels; package boundaries are standard Go package directories such as `exchange/`, `privacy/`, `stored_requests/`, and `util/jsonutil/`.

---

*Convention analysis: 2026-04-05*
