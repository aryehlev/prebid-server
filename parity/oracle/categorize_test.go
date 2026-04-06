package oracle

import (
	"testing"

	"github.com/prebid/prebid-server/v4/parity"
	"github.com/stretchr/testify/assert"
)

func TestCategorizeMismatch(t *testing.T) {
	tests := []struct {
		kind     string
		jsonPath string
		expected parity.Domain
	}{
		{"status_code", "", parity.DomainAuctionLifecycle},
		{"body_diff", "$.seatbid[0].bid[0].price", parity.DomainAuctionLifecycle},
		{"endpoint_error", "$.ext.prebid.errors", parity.DomainEndpoints},
		{"config_mismatch", "", parity.DomainConfigBootstrap},
		{"stored_request_diff", "", parity.DomainStoredData},
		{"privacy_enforcement", "", parity.DomainPolicyPrivacyIdentity},
		{"gdpr_signal", "", parity.DomainPolicyPrivacyIdentity},
		{"bidder_request_diff", "", parity.DomainBiddersAdapters},
		{"hook_execution", "", parity.DomainHooksModules},
		{"cache_write", "", parity.DomainCacheSupportFlows},
		{"unknown_kind", "$.some.path", parity.DomainEndpoints},
	}

	for _, tt := range tests {
		t.Run(tt.kind, func(t *testing.T) {
			got := CategorizeMismatch(tt.kind, tt.jsonPath)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestCategorizeMismatchCoversAllDomains(t *testing.T) {
	// Verify that the categorizer can route to every domain in AllDomains.
	reachable := map[parity.Domain]bool{}
	testInputs := []struct {
		kind     string
		jsonPath string
	}{
		{"status_code", "$.seatbid"},
		{"endpoint_error", ""},
		{"config_mismatch", ""},
		{"stored_request", ""},
		{"privacy_diff", ""},
		{"bidder_diff", ""},
		{"hook_diff", ""},
		{"cache_diff", ""},
		// cutover_proof doesn't have a natural mismatch kind yet
	}

	for _, in := range testInputs {
		d := CategorizeMismatch(in.kind, in.jsonPath)
		reachable[d] = true
	}

	// All domains except cutover_proof should be reachable through categorization.
	for _, d := range parity.AllDomains {
		if d == parity.DomainCutoverProof {
			continue
		}
		assert.True(t, reachable[d], "domain %q is not reachable through categorization", d)
	}
}
