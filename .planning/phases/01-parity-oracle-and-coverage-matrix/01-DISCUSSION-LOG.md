# Phase 1: Parity Oracle and Coverage Matrix - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-05
**Phase:** 1-Parity Oracle and Coverage Matrix
**Areas discussed:** Oracle Input Source, Comparison Strictness, Coverage Matrix Granularity, Initial Coverage Target

---

## Oracle Input Source

| Option | Description | Selected |
|--------|-------------|----------|
| Fixtures + live side-by-side | Shared fixtures define the baseline, and a runner compares Go and Rust on the same cases. | ✓ |
| Fixtures only | Faster to start, but weaker against runtime drift and integration differences. | |
| Live side-by-side only | Good realism, but weaker as a stable executable spec and harder to reproduce precisely. | |
| Other | Describe a different approach. | |

**User's choice:** Fixtures + live side-by-side
**Notes:** Fixtures are the base oracle; live comparison remains part of Phase 1 rather than a later add-on.

---

## Comparison Strictness

| Option | Description | Selected |
|--------|-------------|----------|
| Narrow normalization | Normalize only clearly nondeterministic things like seat-bid ordering, warning ordering, timestamps/IDs, and other generated values. Everything else is a mismatch. | ✓ |
| Pragmatic normalization | Also normalize formatting and other low-signal structural differences to reduce noise early. | |
| Near byte-for-byte | Treat almost everything as a mismatch except a tiny explicit allowlist. | |
| Other | Describe a different rule. | |

**User's choice:** Narrow normalization
**Notes:** Existing Go tests already normalize some ordering behavior; the user wants that kept narrow rather than expanded.

---

## Coverage Matrix Granularity

| Option | Description | Selected |
|--------|-------------|----------|
| Domain-level matrix | Track parity by major surface: endpoints, config/bootstrap, stored data, auction lifecycle, policy/privacy/identity, bidders/adapters, hooks/modules, cache/support flows, cutover proof. | ✓ |
| Phase-level matrix | Track only by roadmap phase. Lower overhead, but less useful for finding exact gaps. | |
| Requirement-level matrix | Track each individual v1 requirement separately from day one. Highest precision, more upkeep. | |
| Other | Describe a different structure. | |

**User's choice:** Domain-level matrix
**Notes:** The matrix should be immediately useful as an execution artifact, not just a high-level dashboard.

---

## Initial Coverage Target

| Option | Description | Selected |
|--------|-------------|----------|
| Main auction flow first, extensible harness | Stand up the oracle on the core auction flow first, but design the harness and matrix so other surfaces plug in without redesign. | ✓ |
| All public surfaces immediately | Start broad from day one. More complete, but more likely to slow Phase 1 before the pattern is proven. | |
| Smallest smoke path only | Start with a very narrow slice of `/openrtb2/auction` just to prove plumbing, then widen later. | |
| Other | Describe a different rollout. | |

**User's choice:** Main auction flow first, extensible harness
**Notes:** Phase 1 should prove the pattern without trapping later phases in a throwaway harness.

---

## the agent's Discretion

- Exact storage format for diff artifacts
- Exact implementation structure of the comparison runner
- Exact presentation format of the domain-level matrix

## Deferred Ideas

None
