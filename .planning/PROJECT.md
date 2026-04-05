# Prebid Server Rust Parity Port

## What This Is

This project is to finish porting the existing Go Prebid Server codebase to Rust under `rust/`. The goal is a Rust implementation that can replace the Go server for production traffic while preserving the current server's business behavior, configuration shape, and operational expectations. Longer term, this creates an open source Rust Prebid Server base that can later carry client-side code.

## Core Value

The Rust server can replace the Go server for production traffic without changing application behavior.

## Requirements

### Validated

- ✓ The current system serves OpenRTB auction traffic through HTTP endpoints and a central auction orchestration flow — existing Go implementation
- ✓ The current system supports extensive bidder and adapter integration through the established adapter boundary and startup registration flow — existing Go implementation
- ✓ The current system loads host configuration, bidder metadata, and static runtime assets through the existing config and bootstrap path — existing Go implementation
- ✓ The current system already includes modules, hooks, stored-request infrastructure, and cache-related support services that shape runtime behavior — existing Go implementation

### Active

- [ ] Rust auction flow matches the Go server's behavior closely enough to replace it for production traffic
- [ ] Rust caching behavior matches the Go server's cache-related behavior and request flow expectations
- [ ] Every Go adapter and bidder used by the server is ported to Rust with matching business behavior
- [ ] Every Go module and related integration path needed for production replacement is ported to Rust with matching behavior
- [ ] Rust configuration loading and operational behavior remain compatible with the current Go server

### Out of Scope

- New product features — parity work comes first
- Refactors that intentionally change Go behavior — exact behavior match is the priority even when the Go design is awkward
- Deployment and tooling cleanup — not required to finish app-code and business-feature parity
- Documentation cleanup — does not move the parity target
- Performance and reliability tuning beyond parity validation — deferred until after feature parity is complete

## Context

This is a brownfield port of the existing Prebid Server repository. The Go codebase remains the canonical behavior reference, while the Rust work lives under `rust/`. Existing codebase analysis shows the Go server already has a centralized auction core, broad adapter and module surfaces, configuration and bidder metadata loading, stored-request infrastructure, and cache-related support services. The current parity gaps called out for the Rust port are caching, auction flow, adapters, bidders, and modules.

The immediate audience for this effort is the team building and operating an open source Rust Prebid Server. The follow-on motivation is to have a Rust server base that can later incorporate prebid client code, but that extension is not part of the current project scope.

## Constraints

- **Behavior**: Rust behavior must match the Go server exactly, even when the Go design is awkward — parity is the objective
- **Compatibility**: Configuration and operational behavior must remain compatible with the existing Go server — replacement depends on it
- **Scope**: Work should focus on app code and business features first — non-functional tuning happens later
- **Reference Implementation**: The Go codebase is the source of truth for expected behavior during the port — parity decisions resolve against it

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Build the parity target in `rust/` against the existing Go repository | Keeps the canonical implementation available as the behavior reference during the port | — Pending |
| Require production-replacement parity, not partial feature coverage | The project only succeeds when the Rust server can stand in for the Go server | — Pending |
| Port every bidder, adapter, and module rather than only a production subset | The stated goal is full server parity, not a deployment-specific slice | — Pending |
| Defer performance, reliability, deployment/tooling cleanup, and docs cleanup | These do not directly complete business-feature parity and would dilute focus | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition**:
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone**:
1. Full review of all sections
2. Core Value check - still the right priority?
3. Audit Out of Scope - reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-05 after initialization*
