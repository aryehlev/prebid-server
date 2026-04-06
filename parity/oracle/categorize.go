package oracle

import (
	"strings"

	"github.com/prebid/prebid-server/v4/parity"
)

// CategorizeMismatch assigns a domain to a mismatch based on its kind and json_path.
func CategorizeMismatch(kind, jsonPath string) parity.Domain {
	lk := strings.ToLower(kind)
	lp := strings.ToLower(jsonPath)

	switch {
	case strings.Contains(lk, "endpoint"):
		return parity.DomainEndpoints
	case strings.Contains(lk, "status") || strings.Contains(lp, "seatbid") || strings.Contains(lp, "bid"):
		return parity.DomainAuctionLifecycle
	case strings.Contains(lk, "config") || strings.Contains(lk, "bootstrap"):
		return parity.DomainConfigBootstrap
	case strings.Contains(lk, "stored"):
		return parity.DomainStoredData
	case strings.Contains(lk, "privacy") || strings.Contains(lk, "gdpr") || strings.Contains(lk, "consent"):
		return parity.DomainPolicyPrivacyIdentity
	case strings.Contains(lk, "bidder") || strings.Contains(lk, "adapter"):
		return parity.DomainBiddersAdapters
	case strings.Contains(lk, "hook") || strings.Contains(lk, "module"):
		return parity.DomainHooksModules
	case strings.Contains(lk, "cache"):
		return parity.DomainCacheSupportFlows
	default:
		return parity.DomainEndpoints
	}
}
