// Package oracle implements the Go-vs-Rust parity comparison oracle.
package oracle

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/prebid/prebid-server/v4/parity"
)

// ComparisonResult holds the output of a single Go-vs-Rust case comparison.
type ComparisonResult struct {
	CaseID     string                   `json:"case_id"`
	Fixture    string                   `json:"fixture_path"`
	Endpoint   string                   `json:"endpoint"`
	GoStatus   int                      `json:"go_status"`
	RsStatus   int                      `json:"rs_status"`
	Mismatches []parity.MismatchRecord  `json:"mismatches"`
}

// RunAuctionOracle runs the parity oracle across the provided fixture paths and
// writes per-case summaries plus an index under outDir.
func RunAuctionOracle(fixturePaths []string, outDir string) (*parity.AuctionIndex, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("create output dir: %w", err)
	}

	idx := &parity.AuctionIndex{}

	for _, fp := range fixturePaths {
		if err := validateFixturePath(fp); err != nil {
			return nil, err
		}

		caseID := caseIDFromPath(fp)

		result, err := runSingleCase(fp, caseID)
		if err != nil {
			return nil, fmt.Errorf("case %s: %w", caseID, err)
		}

		summaryPath := filepath.Join(outDir, caseID+"-summary.json")
		artifactPath := filepath.Join(outDir, caseID+"-artifact.json")

		if err := writeJSON(summaryPath, result); err != nil {
			return nil, fmt.Errorf("write summary: %w", err)
		}
		if err := writeJSON(artifactPath, result); err != nil {
			return nil, fmt.Errorf("write artifact: %w", err)
		}

		kinds := make([]string, 0, len(result.Mismatches))
		domains := make([]parity.Domain, 0)
		domainSeen := map[parity.Domain]bool{}
		for _, m := range result.Mismatches {
			kinds = append(kinds, m.Kind)
			if !domainSeen[m.Domain] {
				domains = append(domains, m.Domain)
				domainSeen[m.Domain] = true
			}
		}

		idx.Cases = append(idx.Cases, parity.AuctionIndexEntry{
			CaseID:        caseID,
			FixturePath:   fp,
			Endpoint:      "/openrtb2/auction",
			Domains:       domains,
			MismatchKinds: kinds,
			SummaryPath:   summaryPath,
			ArtifactPath:  artifactPath,
		})
	}

	indexPath := filepath.Join(outDir, "index.json")
	if err := writeJSON(indexPath, idx); err != nil {
		return nil, fmt.Errorf("write index: %w", err)
	}

	return idx, nil
}

// validateFixturePath rejects any path that escapes the approved sample-request root.
func validateFixturePath(p string) error {
	cleaned := filepath.Clean(p)
	if !strings.HasPrefix(cleaned, parity.SampleRequestRoot) {
		return fmt.Errorf("fixture path %q escapes approved root %q", p, parity.SampleRequestRoot)
	}
	return nil
}

// runSingleCase executes a single parity comparison. This is the stub seam that
// later plans will replace with live Go-vs-Rust execution.
func runSingleCase(fixturePath, caseID string) (*ComparisonResult, error) {
	return &ComparisonResult{
		CaseID:     caseID,
		Fixture:    fixturePath,
		Endpoint:   "/openrtb2/auction",
		GoStatus:   200,
		RsStatus:   200,
		Mismatches: nil, // stub: no mismatches in Wave-0
	}, nil
}

func caseIDFromPath(p string) string {
	base := filepath.Base(p)
	ext := filepath.Ext(base)
	if ext != "" {
		base = base[:len(base)-len(ext)]
	}
	return base
}

func writeJSON(path string, v interface{}) error {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}
