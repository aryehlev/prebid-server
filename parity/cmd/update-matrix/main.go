// Command update-matrix reads the oracle index and generates the compatibility
// matrix snapshots in parity/matrix/.
package main

import (
	"fmt"
	"os"

	"github.com/prebid/prebid-server/v4/parity/matrix"
)

const (
	indexPath    = "parity/out/auction/index.json"
	jsonOutPath  = "parity/matrix/status.json"
	mdOutPath    = "parity/matrix/README.md"
)

func main() {
	rows, err := matrix.BuildMatrix(indexPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "build matrix: %v\n", err)
		os.Exit(1)
	}

	if err := matrix.RenderJSON(rows, jsonOutPath); err != nil {
		fmt.Fprintf(os.Stderr, "render JSON: %v\n", err)
		os.Exit(1)
	}

	if err := matrix.RenderMarkdown(rows, mdOutPath); err != nil {
		fmt.Fprintf(os.Stderr, "render Markdown: %v\n", err)
		os.Exit(1)
	}

	fmt.Printf("Matrix updated: %s, %s\n", jsonOutPath, mdOutPath)
}
