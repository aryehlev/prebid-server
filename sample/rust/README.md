# Rust Prebid Server — sample config

This directory contains a minimal configuration for running the Rust port of
Prebid Server. It is NOT a drop-in replacement for the Go `sample/` layout —
the Rust server consumes its config through `pbs-config::top::Configuration`
(see `rust/crates/config/src/top.rs`), which has a different field layout
than the Go `config.Configuration`.

## Running

From a local build of the workspace:

```sh
cargo run -p prebid-server -- --config sample/rust/pbs.yaml
```

From the Docker image built with `Dockerfile.rust`:

```sh
docker build -f Dockerfile.rust -t prebid-server-rust .
docker run --rm -p 8000:8000 -p 6060:6060 prebid-server-rust
```

The container embeds this file at `/etc/prebid/pbs.yaml` and launches with
`--config /etc/prebid/pbs.yaml` by default.

## Environment overrides

The Rust loader (`pbs-config::env_config`) merges environment variables on
top of the YAML. Supported variables include:

- `PBS_HOST` — override `host`
- `PBS_PORT` — override `port`
- `PBS_ADMIN_PORT` — override `admin_port`
- `PBS_EXTERNAL_URL` — override `external_url`
- `PBS_GDPR_ENABLED` — override `gdpr.enabled`
- `PBS_METRICS_TYPE` — override `metrics.type`

Nested fields follow the `PBS_<SECTION>_<FIELD>` convention.

## How this differs from the Go sample

- The Go sample lives at `sample/001_banner/app.yaml` and is structured around
  stored-request fixtures used by end-to-end tests. This file is a server
  runtime config, not a fixture bundle.
- The Rust config uses `snake_case` keys everywhere (no YAML aliasing).
- `gdpr.default_value` is a string (`"0"` / `"1"`) to match the Go wire type.
- `stored_requests.backend.type` is the discriminator for the backend enum;
  unset or `"none"` disables stored requests entirely.
