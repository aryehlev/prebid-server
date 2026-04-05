# Testing Patterns

**Analysis Date:** 2026-04-05

## Test Framework

**Runner:**
- Go built-in `testing` package on Go `1.23.0` from `go.mod`.
- Config: no dedicated `jest`, `vitest`, or alternate Go test runner config is present; test entrypoints are `Makefile`, `validate.sh`, `scripts/coverage.sh`, and `scripts/check_coverage.sh`.

**Assertion Library:**
- `github.com/stretchr/testify/assert` is the default assertion layer.
- `github.com/stretchr/testify/require` is used for preconditions and fatal setup failures in a smaller subset of files such as `util/jsonutil/merge_test.go`, `server/server_test.go`, and `analytics/pubstack/configupdate_test.go`.

**Run Commands:**
```bash
make test                         # deps + validate.sh
./validate.sh                     # format, go test, optional race/vet/coverage
./validate.sh --race 10           # only TestRace.* tests under the race detector
./scripts/coverage.sh --html      # build package coverage profile and open HTML report
```

## Test File Organization

**Location:**
- Tests are co-located with the packages they exercise. Examples: `config/config_test.go`, `exchange/exchange_test.go`, `endpoints/openrtb2/auction_test.go`, `util/jsonutil/merge_test.go`.
- Package-scoped test helpers also stay beside the tests: `endpoints/openrtb2/test_utils.go`, `adservertargeting/test_data.go`, `hooks/hookexecution/mocks_test.go`.

**Naming:**
- Use `_test.go` suffix for unit, integration-style, race, and benchmark tests.
- Use descriptive file names tied to the production file or subsystem: `privacy/policyenforcer_test.go`, `stored_requests/backends/db_fetcher/fetcher_test.go`, `endpoints/openrtb2/auction_benchmark_test.go`.

**Structure:**
```text
package/
  feature.go
  feature_test.go
  test_utils.go        # optional test helper file in the same package
```

## Test Structure

**Suite Organization:**
```go
tests := []struct {
	description string
	input       SomeType
	expected    OtherType
}{
	{description: "valid input", input: valid, expected: want},
}

for _, tt := range tests {
	t.Run(tt.description, func(t *testing.T) {
		actual := subject(tt.input)
		assert.Equal(t, tt.expected, actual, tt.description)
	})
}
```

Pattern sources: `config/config_test.go`, `stored_requests/events/database/database_test.go`, `analytics/agma/agma_module_test.go`.

**Patterns:**
- Prefer table-driven tests with `[]struct` plus `t.Run(...)`.
- Use `t.Helper()` in reusable assertions and setup helpers: `endpoints/openrtb2/auction_test.go`, `router/router_test.go`, `analytics/pubstack/pubstack_module_test.go`.
- Keep setup close to the test unless a helper removes repeated boilerplate. Helper constructors like `newFetcher(...)` in `stored_requests/backends/db_fetcher/fetcher_test.go` are common.
- `TestMain` is used only for package-wide setup that cannot be repeated cheaply, such as JSON extension registration in `endpoints/openrtb2/auction_test.go` and validator setup in `openrtb_ext/bidders_validate_test.go`.
- `t.Parallel()` is not part of the current repo style; none of the sampled test files use it.

## Mocking

**Framework:** `testify/mock`, `httptest`, `go-sqlmock`, and hand-written fakes

**Patterns:**
```go
type MockedSender struct {
	mock.Mock
}

func (m *MockedSender) Send(payload []byte) error {
	args := m.Called(payload)
	return args.Error(0)
}
```

Source: `analytics/agma/agma_module_test.go`.

```go
server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
}))
defer server.Close()
```

Source: `analytics/agma/sender_test.go`, `floors/fetcher_test.go`, `analytics/pubstack/config_test.go`.

```go
dbMock.ExpectQuery(fakeQueryRegex()).WillReturnRows(tt.giveMockRows)
```

Source: `stored_requests/events/database/database_test.go`, `stored_requests/backends/db_fetcher/fetcher_test.go`.

**What to Mock:**
- External HTTP services with `httptest.NewServer` and `httptest.NewRecorder`.
- SQL/database dependencies with `go-sqlmock`.
- Time, channels, metrics, and module callbacks with narrow fakes or `testify/mock`: `FakeTime` in `stored_requests/events/database/database_test.go`, `MockLogger` in `analytics/filesystem/file_module_test.go`, metrics mocks in `stored_requests/events/database/database_test.go`.

**What NOT to Mock:**
- Pure transformation helpers are usually tested directly with real structs and literals, for example `util/jsonutil/merge_test.go`, `config/events_test.go`, `util/sliceutil/slices_test.go`.
- The repo does not use `testify/suite`; keep tests as plain functions.

## Fixtures and Factories

**Test Data:**
```go
fileData, err := os.ReadFile(filename)
if !assert.NoError(t, err) {
	return
}
test, err := parseTestData(fileData, filename)
```

Source: `endpoints/openrtb2/auction_test.go`.

**Location:**
- JSON and YAML fixtures live beside the package that consumes them, for example `endpoints/openrtb2/sample-requests/...`, `config/test/bidder-info-valid/stroeerCore.yaml`, and DB test assets under `stored_requests/backends/db_provider/test_assets/`.
- Reusable in-memory fixtures are declared as package globals in the test file when they are simple enough: `mockValidAuctionObject` in `analytics/agma/agma_module_test.go`, `bidderInfos` in `config/config_test.go`.

## Coverage

**Requirements:** No hard failing global threshold is enforced by the scripts. `scripts/check_coverage.sh` warns when a package drops below `30%`, but it does not exit non-zero solely for low coverage.

**View Coverage:**
```bash
./scripts/coverage.sh
./scripts/coverage.sh --html
```

## Test Types

**Unit Tests:**
- Dominant test type. Most packages exercise pure functions or package APIs directly in the same package: `util/jsonutil/merge_test.go`, `config/account_test.go`, `privacy/gpp/sid_test.go`.

**Integration Tests:**
- Use lightweight local integrations instead of external services.
- HTTP integration-style tests use `httptest` against real handlers: `router/router_test.go`, `stored_requests/events/api/api_test.go`, `endpoints/openrtb2/auction_test.go`.
- Database integration-style tests use SQL driver mocks rather than a real database: `stored_requests/backends/db_fetcher/fetcher_test.go`, `stored_requests/events/database/database_test.go`.

**E2E Tests:**
- Not used as a separate framework. The closest equivalent is the JSON-sample request harness in `endpoints/openrtb2/auction_test.go`, which drives full handler flows with fixture files and mock bidder servers.

## Common Patterns

**Async Testing:**
```go
select {
case saves = <-eventProducer.Saves():
case <-time.After(20 * time.Millisecond):
}
```

Source: `stored_requests/events/database/database_test.go`.

- Race-focused concurrency tests are isolated behind `TestRace...` names and are only run when `validate.sh` is invoked with `--race`: `analytics/agma/agma_module_test.go`.
- Benchmarks use the standard `Benchmark...` form and often reuse `httptest` infrastructure: `endpoints/openrtb2/auction_benchmark_test.go`, `macros/string_index_based_replacer_test.go`, `exchange/bidder_test.go`.

**Error Testing:**
```go
err := MergeClone(imp, []byte(`{"banner":nul}`))
require.EqualError(t, err, "cannot unmarshal openrtb2.Imp.Banner: expect ull")
```

Source: `util/jsonutil/merge_test.go`.

- Use `assert.EqualError`, `require.EqualError`, `assert.Error`, and `assert.NoError` rather than comparing booleans manually.
- Older tests still use `t.Fatalf` and `t.Errorf` for direct failure paths, especially in setup-heavy code such as `main_test.go` and `analytics/filesystem/file_module_test.go`.

---

*Testing analysis: 2026-04-05*
