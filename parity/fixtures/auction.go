// Package fixtures loads Go test fixtures into parity-owned types.
package fixtures

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/prebid/prebid-server/v4/parity"
)

// LoadAuctionCase reads a Go sample-request JSON fixture and parses it into
// the parity-owned AuctionCase contract. The fixture path must be within
// SampleRequestRoot; any path that escapes the root is rejected.
func LoadAuctionCase(fixturePath string) (parity.AuctionCase, error) {
	cleaned := filepath.Clean(fixturePath)
	if !strings.HasPrefix(cleaned, parity.SampleRequestRoot) {
		return parity.AuctionCase{}, fmt.Errorf(
			"fixture path %q escapes approved root %q", fixturePath, parity.SampleRequestRoot)
	}

	data, err := os.ReadFile(cleaned)
	if err != nil {
		return parity.AuctionCase{}, fmt.Errorf("read fixture %q: %w", cleaned, err)
	}

	var c parity.AuctionCase
	if err := json.Unmarshal(data, &c); err != nil {
		return parity.AuctionCase{}, fmt.Errorf("parse fixture %q: %w", cleaned, err)
	}

	return c, nil
}
