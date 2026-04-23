// Package parity provides shared types and constants for Go-vs-Rust parity comparison.
package parity

import "encoding/json"

// Domain represents a parity surface area aligned with the project roadmap.
type Domain string

const (
	DomainEndpoints             Domain = "endpoints"
	DomainConfigBootstrap       Domain = "config_bootstrap"
	DomainStoredData            Domain = "stored_data"
	DomainAuctionLifecycle      Domain = "auction_lifecycle"
	DomainPolicyPrivacyIdentity Domain = "policy_privacy_identity"
	DomainBiddersAdapters       Domain = "bidders_adapters"
	DomainHooksModules          Domain = "hooks_modules"
	DomainCacheSupportFlows     Domain = "cache_support_flows"
	DomainCutoverProof          Domain = "cutover_proof"
)

// AllDomains is the locked domain list in roadmap order. Tests fail if this
// list is reordered or any entry is removed.
var AllDomains = []Domain{
	DomainEndpoints,
	DomainConfigBootstrap,
	DomainStoredData,
	DomainAuctionLifecycle,
	DomainPolicyPrivacyIdentity,
	DomainBiddersAdapters,
	DomainHooksModules,
	DomainCacheSupportFlows,
	DomainCutoverProof,
}

// SurfaceStatus describes how far parity verification has progressed for a domain.
type SurfaceStatus string

const (
	StatusPassing             SurfaceStatus = "passing"
	StatusMismatch            SurfaceStatus = "mismatch"
	StatusNotYetInstrumented  SurfaceStatus = "not-yet-instrumented"
)

// SampleRequestRoot is the only directory from which parity fixtures may be loaded.
// Any path that escapes this prefix must be rejected before the case is accepted.
const SampleRequestRoot = "endpoints/openrtb2/sample-requests"

// EvidenceLink connects a matrix row to the oracle artifact that supports it.
type EvidenceLink struct {
	CaseID       string `json:"case_id"`
	ArtifactPath string `json:"artifact_path"`
	SummaryPath  string `json:"summary_path"`
}

// MismatchRecord is a structured description of a single Go-vs-Rust difference.
type MismatchRecord struct {
	Domain   Domain `json:"domain"`
	Kind     string `json:"kind"`
	JSONPath string `json:"json_path"`
	GoValue  string `json:"go_value,omitempty"`
	RsValue  string `json:"rs_value,omitempty"`
	Summary  string `json:"summary"`
}

// MatrixRow is one row in the domain-level compatibility matrix.
type MatrixRow struct {
	Domain        Domain        `json:"domain"`
	Status        SurfaceStatus `json:"status"`
	Summary       string        `json:"summary"`
	EvidenceLinks []EvidenceLink `json:"evidence_links"`
}

// AuctionCase holds the inputs and expected outputs parsed from a Go test fixture.
type AuctionCase struct {
	Description                string                        `json:"description"`
	BidRequest                 json.RawMessage               `json:"mockBidRequest"`
	ExpectedReturnCode         int                           `json:"expectedReturnCode,omitempty"`
	ExpectedErrorMessage       string                        `json:"expectedErrorMessage,omitempty"`
	ExpectedBidResponse        json.RawMessage               `json:"expectedBidResponse,omitempty"`
	ExpectedValidatedBidReq    json.RawMessage               `json:"expectedValidatedBidRequest,omitempty"`
	ExpectedMockBidderRequests map[string]json.RawMessage    `json:"expectedMockBidderRequests,omitempty"`
	Config                     json.RawMessage               `json:"config,omitempty"`
}

// AuctionIndexEntry is one row in parity/out/auction/index.json.
type AuctionIndexEntry struct {
	CaseID        string   `json:"case_id"`
	FixturePath   string   `json:"fixture_path"`
	Endpoint      string   `json:"endpoint"`
	Domains       []Domain `json:"domains"`
	MismatchKinds []string `json:"mismatch_kinds"`
	SummaryPath   string   `json:"summary_path"`
	ArtifactPath  string   `json:"artifact_path"`
}

// AuctionIndex is the top-level schema for parity/out/auction/index.json.
type AuctionIndex struct {
	Cases []AuctionIndexEntry `json:"cases"`
}
