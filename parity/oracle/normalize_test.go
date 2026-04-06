package oracle

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNormalizeAuctionResult_WarningOrder(t *testing.T) {
	// Warnings in different orders should normalize to the same output.
	input1 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [],
		"ext": {
			"warnings": {
				"appnexus": ["warn-b", "warn-a"],
				"rubicon": ["warn-z", "warn-m"]
			}
		}
	}`)

	input2 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [],
		"ext": {
			"warnings": {
				"appnexus": ["warn-a", "warn-b"],
				"rubicon": ["warn-m", "warn-z"]
			}
		}
	}`)

	norm1, err := NormalizeAuctionResult(input1)
	require.NoError(t, err)

	norm2, err := NormalizeAuctionResult(input2)
	require.NoError(t, err)

	assert.JSONEq(t, string(norm1), string(norm2))
}

func TestNormalizeAuctionResult_SeatBidOrder(t *testing.T) {
	input1 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [
			{"seat": "rubicon", "bid": [{"impid": "imp-1", "price": 1.0}]},
			{"seat": "appnexus", "bid": [{"impid": "imp-1", "price": 2.0}]}
		]
	}`)

	input2 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [
			{"seat": "appnexus", "bid": [{"impid": "imp-1", "price": 2.0}]},
			{"seat": "rubicon", "bid": [{"impid": "imp-1", "price": 1.0}]}
		]
	}`)

	norm1, err := NormalizeAuctionResult(input1)
	require.NoError(t, err)

	norm2, err := NormalizeAuctionResult(input2)
	require.NoError(t, err)

	assert.JSONEq(t, string(norm1), string(norm2))
}

func TestNormalizeAuctionResult_PreservesRealDifferences(t *testing.T) {
	// Different prices should NOT be normalized away.
	input1 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [{"seat": "appnexus", "bid": [{"impid": "imp-1", "price": 1.0}]}]
	}`)

	input2 := json.RawMessage(`{
		"id": "resp-1",
		"seatbid": [{"seat": "appnexus", "bid": [{"impid": "imp-1", "price": 9.99}]}]
	}`)

	norm1, err := NormalizeAuctionResult(input1)
	require.NoError(t, err)

	norm2, err := NormalizeAuctionResult(input2)
	require.NoError(t, err)

	// These should NOT be equal after normalization
	assert.NotEqual(t, string(norm1), string(norm2))
}

func TestNormalizeAuctionResult_EmptyInput(t *testing.T) {
	result, err := NormalizeAuctionResult(nil)
	require.NoError(t, err)
	assert.Nil(t, result)

	result, err = NormalizeAuctionResult(json.RawMessage{})
	require.NoError(t, err)
	assert.Empty(t, result)
}
