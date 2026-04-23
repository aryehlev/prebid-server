---
phase: 01
slug: parity-oracle-and-coverage-matrix
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-05
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Mixed Go `testing` plus Rust `cargo test` / libtest |
| **Config file** | `go.mod`, `validate.sh`, and `rust/Cargo.toml` |
| **Quick run command** | `go test ./exchange ./endpoints/openrtb2 && cargo test --manifest-path rust/Cargo.toml -p pbs-exchange -p pbs-endpoints` |
| **Full suite command** | `./validate.sh && cargo test --manifest-path rust/Cargo.toml --workspace` |
| **Estimated runtime** | ~180 seconds |

---

## Sampling Rate

- **After every task commit:** Run `go test ./parity/... -count=1`
- **After every plan wave:** Run `go test ./exchange ./endpoints/openrtb2 && cargo test --manifest-path rust/Cargo.toml -p pbs-exchange -p pbs-endpoints`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 0 | PARI-01 | T-01-01 | Fixture corpus stays inside approved directories and case loading rejects arbitrary file traversal | integration | `go test ./parity/... -run TestAuctionOracle -count=1` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 0 | PARI-01 | T-01-02 | Oracle emits categorized mismatches without over-normalizing real behavior differences | integration | `go test ./parity/... -run TestAuctionOracle -count=1` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 0 | PARI-02 | T-01-03 | Matrix rows carry domain labels and evidence links without leaking raw sensitive request data into committed summaries | unit | `go test ./parity/... -run TestCoverageMatrix -count=1` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `parity/oracle/auction_test.go` — stubs for PARI-01
- [ ] `parity/oracle/categorize_test.go` — shared mismatch taxonomy assertions for PARI-01
- [ ] `parity/matrix/matrix_test.go` — matrix completeness and evidence-link checks for PARI-02
- [ ] `parity/testdata/` — approved normalization and categorization samples

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Domain matrix readability for later planning use | PARI-02 | Automated checks can prove completeness, but not whether the report is actually easy to route into later phases | Open the generated matrix/report artifact, confirm every domain row exists, and verify a human can map each mismatch to a roadmap domain without guesswork |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
