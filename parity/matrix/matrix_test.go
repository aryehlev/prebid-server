package matrix

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/prebid/prebid-server/v4/parity"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCoverageMatrix(t *testing.T) {
	// Wave-0: verify that BuildMatrix produces all locked domain rows in order
	// even when no oracle evidence exists.
	rows, err := BuildMatrix("")
	require.NoError(t, err)
	require.Len(t, rows, len(parity.AllDomains), "matrix must have exactly one row per locked domain")

	for i, row := range rows {
		assert.Equal(t, parity.AllDomains[i], row.Domain,
			"row %d domain mismatch: got %q want %q", i, row.Domain, parity.AllDomains[i])
		assert.Equal(t, parity.StatusNotYetInstrumented, row.Status,
			"row %d (%s) should be not-yet-instrumented with no evidence", i, row.Domain)
		assert.NotEmpty(t, row.Summary)
	}
}

func TestCoverageMatrixLockedDomainOrder(t *testing.T) {
	// Verify the exact locked domain order matches the canonical fixture.
	expected := []parity.Domain{
		"endpoints",
		"config_bootstrap",
		"stored_data",
		"auction_lifecycle",
		"policy_privacy_identity",
		"bidders_adapters",
		"hooks_modules",
		"cache_support_flows",
		"cutover_proof",
	}
	assert.Equal(t, expected, parity.AllDomains)
}

func TestCoverageMatrixWithEvidence(t *testing.T) {
	// Create a temporary index with evidence for endpoints and auction_lifecycle.
	idx := parity.AuctionIndex{
		Cases: []parity.AuctionIndexEntry{
			{
				CaseID:        "test-case-1",
				FixturePath:   "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:      "/openrtb2/auction",
				Domains:       []parity.Domain{parity.DomainEndpoints, parity.DomainAuctionLifecycle},
				MismatchKinds: []string{},
				SummaryPath:   "parity/out/auction/test-case-1-summary.json",
				ArtifactPath:  "parity/out/auction/test-case-1-artifact.json",
			},
		},
	}

	tmpDir := t.TempDir()
	indexPath := filepath.Join(tmpDir, "index.json")
	data, err := json.MarshalIndent(idx, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(indexPath, data, 0o644))

	rows, err := BuildMatrix(indexPath)
	require.NoError(t, err)
	require.Len(t, rows, len(parity.AllDomains))

	// Endpoints and auction_lifecycle should be passing.
	assert.Equal(t, parity.StatusPassing, rows[0].Status, "endpoints should be passing")
	assert.Equal(t, parity.DomainEndpoints, rows[0].Domain)
	assert.Len(t, rows[0].EvidenceLinks, 1)

	assert.Equal(t, parity.StatusPassing, rows[3].Status, "auction_lifecycle should be passing")
	assert.Equal(t, parity.DomainAuctionLifecycle, rows[3].Domain)
	assert.Len(t, rows[3].EvidenceLinks, 1)

	// All other domains should be not-yet-instrumented.
	for _, i := range []int{1, 2, 4, 5, 6, 7, 8} {
		assert.Equal(t, parity.StatusNotYetInstrumented, rows[i].Status,
			"domain %s should be not-yet-instrumented", rows[i].Domain)
		assert.Empty(t, rows[i].EvidenceLinks)
	}
}

func TestCoverageMatrixWithMismatches(t *testing.T) {
	idx := parity.AuctionIndex{
		Cases: []parity.AuctionIndexEntry{
			{
				CaseID:        "mismatch-case",
				FixturePath:   "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:      "/openrtb2/auction",
				Domains:       []parity.Domain{parity.DomainEndpoints},
				MismatchKinds: []string{"status_code"},
				SummaryPath:   "parity/out/auction/mismatch-case-summary.json",
				ArtifactPath:  "parity/out/auction/mismatch-case-artifact.json",
			},
		},
	}

	tmpDir := t.TempDir()
	indexPath := filepath.Join(tmpDir, "index.json")
	data, err := json.MarshalIndent(idx, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(indexPath, data, 0o644))

	rows, err := BuildMatrix(indexPath)
	require.NoError(t, err)

	assert.Equal(t, parity.StatusMismatch, rows[0].Status, "endpoints should show mismatch")
}

func TestCoverageMatrixRejectsIncompleteEntries(t *testing.T) {
	tests := []struct {
		name  string
		entry parity.AuctionIndexEntry
	}{
		{
			name: "missing case_id",
			entry: parity.AuctionIndexEntry{
				FixturePath:  "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:     "/openrtb2/auction",
				SummaryPath:  "s.json",
				ArtifactPath: "a.json",
			},
		},
		{
			name: "missing summary_path",
			entry: parity.AuctionIndexEntry{
				CaseID:       "x",
				FixturePath:  "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:     "/openrtb2/auction",
				ArtifactPath: "a.json",
			},
		},
		{
			name: "missing artifact_path",
			entry: parity.AuctionIndexEntry{
				CaseID:      "x",
				FixturePath: "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:    "/openrtb2/auction",
				SummaryPath: "s.json",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			idx := parity.AuctionIndex{Cases: []parity.AuctionIndexEntry{tt.entry}}
			tmpDir := t.TempDir()
			indexPath := filepath.Join(tmpDir, "index.json")
			data, _ := json.MarshalIndent(idx, "", "  ")
			os.WriteFile(indexPath, data, 0o644)

			_, err := BuildMatrix(indexPath)
			assert.Error(t, err)
		})
	}
}

func TestCoverageMatrixEvidenceLinkFields(t *testing.T) {
	idx := parity.AuctionIndex{
		Cases: []parity.AuctionIndexEntry{
			{
				CaseID:        "evidence-check",
				FixturePath:   "endpoints/openrtb2/sample-requests/valid-whole/exemplary/simple.json",
				Endpoint:      "/openrtb2/auction",
				Domains:       []parity.Domain{parity.DomainEndpoints},
				MismatchKinds: []string{},
				SummaryPath:   "parity/out/auction/evidence-check-summary.json",
				ArtifactPath:  "parity/out/auction/evidence-check-artifact.json",
			},
		},
	}

	tmpDir := t.TempDir()
	indexPath := filepath.Join(tmpDir, "index.json")
	data, _ := json.MarshalIndent(idx, "", "  ")
	os.WriteFile(indexPath, data, 0o644)

	rows, err := BuildMatrix(indexPath)
	require.NoError(t, err)

	// Verify evidence link fields are present.
	link := rows[0].EvidenceLinks[0]
	assert.Equal(t, "evidence-check", link.CaseID)
	assert.NotEmpty(t, link.ArtifactPath)
	assert.NotEmpty(t, link.SummaryPath)
}
