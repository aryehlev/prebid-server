# Technology Stack

**Project:** Prebid Server Rust parity port
**Research focus:** Production-capable OpenRTB bidding server that preserves Go Prebid Server behavior
**Researched:** 2026-04-05

## Recommendation Summary

Use the standard current Tokio HTTP stack, but constrain it with parity-first rules:

- `tokio` + `axum` + `tower` + `tower-http` for the server runtime and middleware surface.
- `reqwest` with `rustls` for outbound bidder/cache/service calls.
- `serde`/`serde_json`/`serde_yaml` plus a custom compatibility loader for config and JSON/YAML parity.
- Direct `prometheus` instrumentation for metrics parity; use `tracing` for logs and spans.
- `jsonschema` for bidder params and static schema validation.
- `moka` only for in-process bounded caches; `sqlx` only where Go-compatible MySQL/Postgres stored-request paths are required.

This is not a greenfield Rust service. Go compatibility is the dominant constraint, so several normal Rust preferences should be rejected if they change wire behavior, config precedence, JSON encoding, metric names, timeout behavior, or adapter/module execution order.

## Recommended Stack

### Core Runtime And HTTP

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| Rust toolchain | `1.94.1` stable, edition `2024` | Compiler, Cargo, clippy, rustfmt | Current stable Rust as of 2026-04-05. Edition 2024 is already stable; pinning the toolchain removes CI/operator drift. | HIGH |
| `tokio` | `1.50.x` | Async runtime, timers, tasks, sockets, signals | Standard current Rust async runtime. It is the runtime `axum`, `reqwest`, and `sqlx` already target. | HIGH |
| `axum` | `0.8.x` | HTTP routing and handler composition | Current mainstream Tokio-native web framework with predictable handler model and first-class Tower integration. Best fit for endpoint-heavy PBS routing without inventing framework glue. | HIGH |
| `tower` | `0.5.x` | Middleware and service composition | Gives explicit request pipeline control for timeouts, concurrency limits, retries, buffering, and per-route policy. That maps well to PBS’s layered request handling. | HIGH |
| `tower-http` | `0.6.x` | HTTP middleware: CORS, tracing, compression, timeout headers | Standard companion middleware crate for Axum/Tower. Use it for transport concerns only, not business behavior. | HIGH |

### Outbound Networking And TLS

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `reqwest` | `0.13.x` | Shared HTTP client for bidders, cache, currency, stored-request HTTP fetchers | Standard current high-level client on Tokio. Reuse pooled clients per destination class; do not create clients per request. | HIGH |
| `rustls` | `0.23.x` | Default TLS backend | Cross-platform, modern TLS defaults, no OpenSSL dependency chain. Use as the default outbound TLS path. | HIGH |
| `tokio-util::sync::CancellationToken` | `0.7.x` | Cooperative request cancellation across fan-out tasks | Critical for auction fan-out so losing branches stop promptly when deadlines expire or the client disconnects. | HIGH |
| `bytes` + `http` | `1.11.x` + `1.4.x` | Efficient body/header handling | Standard supporting crates for low-allocation request/response plumbing around Axum, Reqwest, and OpenRTB payload handling. | HIGH |

### Serialization, OpenRTB Models, And Compatibility

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `serde` | `1.0.228+` | Core serialization/deserialization | Standard Rust serialization layer. Required for JSON/YAML parity and strongly typed config/models. | HIGH |
| `serde_json` | `1.x` | OpenRTB, ext, stored request/account JSON | Keep JSON behavior explicit with handwritten `serde` annotations and golden fixtures. Do not rely on generic “fast JSON” swaps in parity-critical code. | HIGH |
| `serde_yaml` | `0.9.x` | Host config and bidder metadata YAML | Matches PBS operational inputs: YAML bidder info and YAML/JSON config files. | HIGH |
| Internal `openrtb` crate | workspace crate | Canonical OpenRTB request/response model | Keep the model in-repo so PBS-specific extensions, deprecated video paths, and serialization quirks can be controlled directly. | HIGH |
| Internal `openrtb-ext` crate | workspace crate | PBS-specific OpenRTB ext types and response ext structure | External generic OpenRTB crates are too coarse for full PBS parity. Keep ext ownership local. | HIGH |
| `jsonschema` | `0.45.x` | Bidder params schema validation and startup-time schema checks | Current high-performance validator with support for older drafts that PBS assets may still depend on. Reuse compiled validators. | HIGH |

### Config And Static Asset Loading

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `serde`-based custom config loader | custom crate code over `serde_json`/`serde_yaml` | Reproduce Go PBS config precedence, env binding, defaults, and merge semantics | This is the most important non-obvious stack decision. Generic Rust config frameworks do not guarantee Viper-compatible precedence or env-name behavior. Build an explicit compatibility layer instead. | HIGH |
| `toml` | `0.8.x` only if needed for Rust-native local tooling | Developer-only config support | Useful for Rust-native tool config, but not as a production PBS config format. | MEDIUM |
| `walkdir`/plain `std::fs` style startup loaders | std-first | Load `static/bidder-info`, schemas, mappings, and runtime assets at startup | PBS already depends on startup-loaded disk assets. Preserve that shape instead of pushing metadata into compile-time embedding. | MEDIUM |

### Persistence, Cache, And State

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `moka` | `0.12.x` | Bounded in-process caches for stored requests, schema caches, floors, and similar read-heavy TTL state | Current concurrent cache with async support, TTL/TTI, and bounded eviction. Better fit than ad hoc `DashMap` caches. | HIGH |
| `sqlx` | `0.8.x` | MySQL/Postgres access for stored requests/accounts only where deployments need it | Async, mainstream, supports both Tokio and Rustls/native-tls modes, and keeps SQL explicit. Use narrowly; PBS is not an ORM-heavy app. | HIGH |
| `dashmap` | `6.1.x`, but avoid as primary cache | Small concurrent maps and registries | Fine for registries and low-level shared maps. Do not let it become the caching strategy; use `moka` where eviction/TTL matter. | MEDIUM |
| HTTP integration with Prebid Cache | via `reqwest` | Cache-service compatibility | Preserve the Go architecture: integrate with Prebid Cache over HTTP. Do not replace this with Redis or an embedded cache if the goal is production replacement. | HIGH |

### Observability And Operations

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `tracing` | `0.1.44+` | Structured logs and request-scoped spans | Standard Rust diagnostics layer, especially for async systems. It is the right base for per-request correlation and adapter fan-out diagnostics. | HIGH |
| `tracing-subscriber` | `0.3.23+` | JSON logging, env-filtered verbosity, layered subscribers | Use JSON logs in production and text logs locally. Keep field names stable enough for operator dashboards. | HIGH |
| `prometheus` | `0.14.x` | Direct Prometheus metric registry and scrape output | Best fit for matching current PBS-style Prometheus metrics. Use direct instrumentation rather than hiding semantics behind a metrics facade. | HIGH |
| `opentelemetry` / `opentelemetry-otlp` | `0.31.x`, optional | Trace export into existing OTEL backends | Make this optional and additive. Do not make OTEL metrics the primary metrics surface for parity work. | MEDIUM |
| Glibc-based Linux container image | Debian/Ubuntu slim class image | Production runtime image | Safer default than `scratch`/musl for operational replacement because bidder integrations, CA bundles, and possible native deps are less surprising on glibc. | MEDIUM |

### Testing, Verification, And Performance Tooling

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| `cargo-nextest` | current release | Main test runner in CI | Faster and more reliable than plain `cargo test` for a large multi-crate workspace; supports retries, isolation, and CI-friendly reporting. | HIGH |
| `wiremock` | `0.6.x` | Black-box HTTP mocking for bidder/cache/service integration tests | Strong fit for adapter tests because adapters are HTTP integrations. Prefer this over brittle unit-only mocking. | HIGH |
| `proptest` | `1.11.x` | Property-based testing for OpenRTB normalization, merging, and validation invariants | Valuable for parity work because the request-shape state space is large and edge-case driven. | MEDIUM |
| `criterion` | `0.8.x` | Microbenchmarks for hot-path serialization, fan-out, and validation | Use for targeted regressions, not vanity benchmarking. | HIGH |
| `cargo fmt` + `clippy` | toolchain built-ins | Style and lint enforcement | Standard Rust baseline. Keep the lint set strict because adapter code will grow rapidly. | HIGH |

## Prescriptive Choices

### Use This

- Build the server on `axum 0.8` and `tower 0.5`, not directly on bare `hyper`, unless a very specific transport gap forces it.
- Keep one shared `reqwest::Client` per outbound policy class:
  - bidder traffic
  - cache/service traffic
  - bootstrap/background fetches
- Use `rustls` by default for outbound TLS.
- Use `CancellationToken`, request deadlines, and explicit fan-out budgets for every auction.
- Keep OpenRTB and `ext` models in repo-owned crates.
- Validate bidder params and similar static assets with precompiled `jsonschema` validators at startup.
- Instrument metrics directly with `prometheus` and expose a stable scrape endpoint.
- Pin the Rust toolchain in `rust-toolchain.toml`.

### Do Not Use

| Category | Do Not Use | Why Not |
|----------|------------|---------|
| Web framework | `actix-web`, `warp`, or a framework swap from the current Axum path | No parity upside. It adds migration risk and makes existing Rust work less reusable. |
| HTTP client | Per-request `reqwest::Client` construction | Destroys connection reuse and adds avoidable latency on bidder fan-out. |
| Config | A generic config crate as the source of truth for production semantics | PBS replacement depends on exact precedence and env-name compatibility. Generic merging behavior is not enough. |
| Metrics | `opentelemetry-prometheus` as the main metrics path | The official docs say it is no longer recommended and development is discontinued. |
| Database layer | `diesel` or an ORM-first approach | PBS persistence paths are narrow and operational, not relational-domain heavy. Explicit SQL is easier to keep Go-compatible. |
| Cache integration | Redis as a replacement for Prebid Cache HTTP integration | That changes architecture and operational behavior. Replacement work should preserve the existing service boundary. |
| JSON path | Defaulting to exotic JSON stacks for speed | Parity failures from field omission/order/number handling will cost more than any early micro-optimization win. |
| Plugin model | Runtime dynamic loading for adapters/modules | PBS adapters/modules are operationally in-process and performance-sensitive. Static registration is simpler and safer for parity. |

## Go-Compatibility Constraints That Override Normal Rust Preferences

1. **Config compatibility beats elegance.**
   - Preserve Go PBS precedence: env vars, then `pbs.json`, then `pbs.yaml`, plus the same `PBS_` env-name mapping behavior.
   - Do not accept a cleaner Rust-native config model if it breaks existing deployments.

2. **Wire compatibility beats abstraction purity.**
   - JSON field omission, `null` handling, ext payload shape, error payloads, and response extensions must be locked to Go behavior with fixtures.
   - If a serializer optimization changes wire output, reject it.

3. **Operational compatibility beats “cloud-native minimalism.”**
   - Keep Prometheus scraping, admin/main listeners, disk-loaded static assets, and external cache-service integration unless the Go server behavior being replaced already differs.

4. **Adapter compatibility beats trait cleverness.**
   - Favor a simple static registry and explicit adapter trait boundary over fancy plugin systems or macro-heavy magic.
   - The cost center is correctness across hundreds of bidders, not framework novelty.

5. **Deadline behavior beats maximal concurrency.**
   - Rust makes massive concurrency easy; PBS parity requires bounded, cancelable concurrency with predictable timeout behavior.

## Current Workspace Delta

The existing Rust workspace is directionally correct, but these stack updates are warranted:

| Current workspace | Recommended target | Reason |
|-------------------|--------------------|--------|
| `axum 0.7` | `axum 0.8.x` | Align with current maintained surface. |
| `tower 0.4` | `tower 0.5.x` | Match current Axum ecosystem baseline. |
| `tower-http 0.5` | `tower-http 0.6.x` | Match current middleware baseline. |
| `reqwest 0.12` | `reqwest 0.13.x` | Stay on the current mainstream client release. |
| `prometheus 0.13` | `prometheus 0.14.x` | Current release line. |
| Rust edition `2021` | edition `2024` | Stable now; worth standardizing for new parity work. |

## Installation

```bash
# Workspace baseline
cargo add tokio@1 axum@0.8 tower@0.5 tower-http@0.6 reqwest@0.13 \
  rustls@0.23 tokio-util@0.7 serde@1 serde_json@1 serde_yaml@0.9 \
  tracing@0.1 tracing-subscriber@0.3 prometheus@0.14 jsonschema@0.45 \
  moka@0.12 sqlx@0.8

# Suggested reqwest features
cargo add reqwest@0.13 --features json,gzip,rustls-tls,http2

# SQLx features if DB-backed stored requests/accounts are required
cargo add sqlx@0.8 --features runtime-tokio,tls-rustls,postgres,mysql

# Dev/test tooling
cargo add --dev wiremock@0.6 proptest@1 criterion@0.8
cargo install cargo-nextest
```

## Sources

### Primary Sources

- Rust stable release announcements: `1.94.0` on 2026-03-05 and `1.94.1` on 2026-03-26
  - https://blog.rust-lang.org/2026/03/05/Rust-1.94.0/
  - https://blog.rust-lang.org/2026/03/26/1.94.1-release/
- Rust 2024 stabilization:
  - https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/
- Tokio docs (`tokio 1.50.0`):
  - https://docs.rs/tokio/latest/tokio/
- Axum docs (`axum 0.8.8`):
  - https://docs.rs/axum/latest/axum/
- Tower docs (`tower 0.5.3`):
  - https://docs.rs/tower/latest/tower/
- Tower HTTP docs (`tower-http 0.6.8`):
  - https://docs.rs/tower-http/latest/tower_http/
- Reqwest docs (`reqwest 0.13.2`):
  - https://docs.rs/reqwest/latest/reqwest/
- Bytes docs (`bytes 1.11.1`):
  - https://docs.rs/bytes/latest/bytes/
- HTTP types docs (`http 1.4.0`):
  - https://docs.rs/http/latest/http/
- Rustls docs (`rustls 0.23.37`):
  - https://docs.rs/rustls/latest/rustls/
- Serde docs (`serde 1.0.228`):
  - https://docs.rs/serde/latest/serde/
- Tracing docs (`tracing 0.1.44`):
  - https://docs.rs/tracing/latest/tracing/
- Tracing Subscriber docs (`tracing-subscriber 0.3.23`):
  - https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
- JSON Schema docs (`jsonschema 0.45.0`):
  - https://docs.rs/jsonschema/latest/jsonschema/
- Moka docs (`moka 0.12.15`):
  - https://docs.rs/moka/latest/moka/
- Prometheus Rust client docs (`prometheus 0.14.0`):
  - https://docs.rs/prometheus/latest/prometheus/
- SQLx docs (`sqlx 0.8.6`):
  - https://docs.rs/sqlx/latest/sqlx/
- DashMap docs (`dashmap 6.1.0`):
  - https://docs.rs/dashmap/latest/dashmap/
- Tokio Util `CancellationToken` docs (`tokio-util 0.7.18`):
  - https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html
- cargo-nextest docs:
  - https://nexte.st/
- wiremock docs (`wiremock 0.6.5`):
  - https://docs.rs/wiremock/latest/wiremock/
- Proptest docs (`proptest 1.11.0`):
  - https://docs.rs/proptest/latest/proptest/
- Criterion docs (`criterion 0.8.2`):
  - https://docs.rs/criterion/latest/criterion/
- OpenTelemetry Rust docs:
  - https://opentelemetry.io/docs/languages/rust/
- `opentelemetry-prometheus` docs warning that it is no longer recommended:
  - https://docs.rs/opentelemetry-prometheus/latest/opentelemetry_prometheus/

### Local Project Context

- `.planning/PROJECT.md`
- `.planning/codebase/STACK.md`
- `.planning/codebase/ARCHITECTURE.md`
- `rust/Cargo.toml`

## Confidence Notes

- **HIGH:** Tokio/Axum/Tower/Reqwest/Rustls/Serde/Tracing/Prometheus/JSONSchema/Moka/SQLx recommendations are backed by current official docs and fit the existing Rust workspace direction.
- **MEDIUM:** Glibc container recommendation and some support-crate choices are based on current PBS replacement constraints plus standard Rust ops practice, not a single authoritative crate doc.
- **LOW:** None.
