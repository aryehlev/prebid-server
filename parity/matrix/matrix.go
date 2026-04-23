// Package matrix builds and renders the domain-level parity compatibility matrix.
package matrix

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/prebid/prebid-server/v4/parity"
)

// BuildMatrix reads the oracle index and produces a complete matrix with one
// row per locked domain. Domains covered by oracle evidence get their status
// from the evidence; all other domains are marked not-yet-instrumented.
func BuildMatrix(indexPath string) ([]parity.MatrixRow, error) {
	var idx parity.AuctionIndex

	if indexPath != "" {
		data, err := os.ReadFile(indexPath)
		if err != nil {
			if !os.IsNotExist(err) {
				return nil, fmt.Errorf("read index: %w", err)
			}
			// No index yet — all domains are not-yet-instrumented.
		} else {
			if err := json.Unmarshal(data, &idx); err != nil {
				return nil, fmt.Errorf("parse index: %w", err)
			}
		}
	}

	// Collect evidence per domain from oracle cases.
	domainEvidence := map[parity.Domain][]parity.EvidenceLink{}
	domainHasMismatch := map[parity.Domain]bool{}

	for _, c := range idx.Cases {
		// Validate required fields.
		if c.CaseID == "" || c.FixturePath == "" || c.Endpoint == "" {
			return nil, fmt.Errorf("index entry missing required fields: case_id=%q fixture_path=%q endpoint=%q",
				c.CaseID, c.FixturePath, c.Endpoint)
		}
		if c.SummaryPath == "" || c.ArtifactPath == "" {
			return nil, fmt.Errorf("index entry %q missing evidence-link fields", c.CaseID)
		}

		link := parity.EvidenceLink{
			CaseID:       c.CaseID,
			ArtifactPath: c.ArtifactPath,
			SummaryPath:  c.SummaryPath,
		}

		// Map case to domains it covers.
		caseDomains := c.Domains
		if len(caseDomains) == 0 {
			// Auction cases without explicit domain tags default to endpoints + auction_lifecycle.
			caseDomains = []parity.Domain{parity.DomainEndpoints, parity.DomainAuctionLifecycle}
		}

		for _, d := range caseDomains {
			domainEvidence[d] = append(domainEvidence[d], link)
		}

		if len(c.MismatchKinds) > 0 {
			for _, d := range caseDomains {
				domainHasMismatch[d] = true
			}
		}
	}

	// Build one row per locked domain in AllDomains order.
	rows := make([]parity.MatrixRow, len(parity.AllDomains))
	for i, d := range parity.AllDomains {
		evidence := domainEvidence[d]
		var status parity.SurfaceStatus
		var summary string

		switch {
		case len(evidence) == 0:
			status = parity.StatusNotYetInstrumented
			summary = "No oracle evidence yet for this domain."
		case domainHasMismatch[d]:
			status = parity.StatusMismatch
			summary = fmt.Sprintf("Mismatches detected across %d case(s).", len(evidence))
		default:
			status = parity.StatusPassing
			summary = fmt.Sprintf("Passing across %d case(s).", len(evidence))
		}

		rows[i] = parity.MatrixRow{
			Domain:        d,
			Status:        status,
			Summary:       summary,
			EvidenceLinks: evidence,
		}
	}

	return rows, nil
}
