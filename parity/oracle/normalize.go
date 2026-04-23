package oracle

import (
	"encoding/json"
	"sort"
)

// NormalizeAuctionResult applies the narrow approved normalization rules:
// 1. Sort warning arrays for order-independent comparison
// 2. Sort seatbid arrays by seat for order-independent comparison
// 3. Sort bids within each seatbid by impid then price
//
// Only these specific normalizations are approved. All other differences
// remain real mismatches.
func NormalizeAuctionResult(raw json.RawMessage) (json.RawMessage, error) {
	if len(raw) == 0 {
		return raw, nil
	}

	var obj map[string]json.RawMessage
	if err := json.Unmarshal(raw, &obj); err != nil {
		return raw, nil // not a JSON object, return as-is
	}

	// Normalize ext.warnings / ext.prebid.warnings (warning order)
	if extRaw, ok := obj["ext"]; ok {
		normalized, err := normalizeExtWarnings(extRaw)
		if err == nil {
			obj["ext"] = normalized
		}
	}

	// Normalize seatbid ordering
	if seatbidRaw, ok := obj["seatbid"]; ok {
		normalized, err := normalizeSeatBids(seatbidRaw)
		if err == nil {
			obj["seatbid"] = normalized
		}
	}

	return json.Marshal(obj)
}

func normalizeExtWarnings(extRaw json.RawMessage) (json.RawMessage, error) {
	var ext map[string]json.RawMessage
	if err := json.Unmarshal(extRaw, &ext); err != nil {
		return extRaw, err
	}

	// Sort top-level warnings
	if w, ok := ext["warnings"]; ok {
		sorted, err := sortJSONObject(w)
		if err == nil {
			ext["warnings"] = sorted
		}
	}

	// Sort ext.prebid.warnings
	if prebidRaw, ok := ext["prebid"]; ok {
		var prebid map[string]json.RawMessage
		if err := json.Unmarshal(prebidRaw, &prebid); err == nil {
			if w, ok := prebid["warnings"]; ok {
				sorted, err := sortJSONObject(w)
				if err == nil {
					prebid["warnings"] = sorted
					ext["prebid"], _ = json.Marshal(prebid)
				}
			}
		}
	}

	return json.Marshal(ext)
}

// sortJSONObject sorts string arrays within a JSON object's values.
func sortJSONObject(raw json.RawMessage) (json.RawMessage, error) {
	var obj map[string][]string
	if err := json.Unmarshal(raw, &obj); err != nil {
		// Try as map[string][]json.RawMessage
		var objRaw map[string][]json.RawMessage
		if err2 := json.Unmarshal(raw, &objRaw); err2 != nil {
			return raw, err
		}
		for k, arr := range objRaw {
			strs := make([]string, len(arr))
			for i, v := range arr {
				strs[i] = string(v)
			}
			sort.Strings(strs)
			for i, s := range strs {
				objRaw[k][i] = json.RawMessage(s)
			}
		}
		return json.Marshal(objRaw)
	}
	for k, arr := range obj {
		sort.Strings(arr)
		obj[k] = arr
	}
	return json.Marshal(obj)
}

func normalizeSeatBids(raw json.RawMessage) (json.RawMessage, error) {
	var seatbids []map[string]json.RawMessage
	if err := json.Unmarshal(raw, &seatbids); err != nil {
		return raw, err
	}

	// Sort seatbids by seat name
	sort.Slice(seatbids, func(i, j int) bool {
		si := extractString(seatbids[i]["seat"])
		sj := extractString(seatbids[j]["seat"])
		return si < sj
	})

	// Sort bids within each seatbid by impid then price
	for _, sb := range seatbids {
		if bidRaw, ok := sb["bid"]; ok {
			var bids []map[string]json.RawMessage
			if err := json.Unmarshal(bidRaw, &bids); err == nil {
				sort.Slice(bids, func(i, j int) bool {
					ii := extractString(bids[i]["impid"])
					ij := extractString(bids[j]["impid"])
					if ii != ij {
						return ii < ij
					}
					return string(bids[i]["price"]) < string(bids[j]["price"])
				})
				sb["bid"], _ = json.Marshal(bids)
			}
		}
	}

	return json.Marshal(seatbids)
}

func extractString(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	return string(raw)
}
