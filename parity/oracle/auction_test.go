package oracle

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/prebid/prebid-server/v4/parity"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAuctionOracle(t *testing.T) {
	// Wave-0: exercise the stub oracle seam with an approved fixture path.
	// The stub returns a deterministic placeholder result shaped for later
	// live execution.
	fixtures := []string{
		filepath.Join(parity.SampleRequestRoot, "valid-whole", "exemplary", "simple.json"),
	}

	outDir := t.TempDir()
	idx, err := RunAuctionOracle(fixtures, outDir)
	require.NoError(t, err)
	require.Len(t, idx.Cases, 1)

	entry := idx.Cases[0]
	assert.Equal(t, "simple", entry.CaseID)
	assert.Equal(t, fixtures[0], entry.FixturePath)
	assert.Equal(t, "/openrtb2/auction", entry.Endpoint)
	assert.Empty(t, entry.MismatchKinds, "stub oracle should produce zero mismatches in Wave-0")

	// Verify index.json was written
	indexData, err := os.ReadFile(filepath.Join(outDir, "index.json"))
	require.NoError(t, err)
	assert.Contains(t, string(indexData), "simple")

	// Verify per-case summary was written
	summaryData, err := os.ReadFile(filepath.Join(outDir, "simple-summary.json"))
	require.NoError(t, err)
	assert.Contains(t, string(summaryData), "simple")
}

func TestAuctionOracleRejectsTraversal(t *testing.T) {
	tests := []struct {
		name string
		path string
	}{
		{
			name: "parent directory escape",
			path: "../../../etc/passwd",
		},
		{
			name: "absolute path outside root",
			path: "/tmp/evil.json",
		},
		{
			name: "dot-dot inside approved prefix",
			path: parity.SampleRequestRoot + "/../../../etc/shadow",
		},
		{
			name: "unrelated directory",
			path: "exchange/testdata/some-file.json",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			outDir := t.TempDir()
			_, err := RunAuctionOracle([]string{tt.path}, outDir)
			require.Error(t, err)
			assert.Contains(t, err.Error(), "escapes approved root")
		})
	}
}

func TestAuctionOracleMultipleCases(t *testing.T) {
	fixtures := []string{
		filepath.Join(parity.SampleRequestRoot, "valid-whole", "exemplary", "simple.json"),
		filepath.Join(parity.SampleRequestRoot, "valid-whole", "exemplary", "all-ext.json"),
	}

	outDir := t.TempDir()
	idx, err := RunAuctionOracle(fixtures, outDir)
	require.NoError(t, err)
	require.Len(t, idx.Cases, 2)

	assert.Equal(t, "simple", idx.Cases[0].CaseID)
	assert.Equal(t, "all-ext", idx.Cases[1].CaseID)
}
