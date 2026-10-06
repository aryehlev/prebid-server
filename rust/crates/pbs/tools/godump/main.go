// godump runs the real Prebid Server v3.30.0 adapters (the version rtb-seller-digital pins) on
// the cases it is given and prints what they produce, so the Rust port can be compared with it.
//
// Input (stdin): a JSON array of cases:
//
//	{"id": "...", "bidder": "appnexus", "endpoint": "...", "extra_info": "...", "platform_id": "...",
//	 "request": {...BidRequest...}, "response": {"status": 200, "body": "..."} (optional)}
//
// Output (stdout): a JSON array of results in the same order.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/prebid/openrtb/v20/openrtb2"
	"github.com/prebid/prebid-server/v3/adapters"
	"github.com/prebid/prebid-server/v3/config"
	"github.com/prebid/prebid-server/v3/currency"
	"github.com/prebid/prebid-server/v3/openrtb_ext"
)

type response struct {
	Status int     `json:"status"`
	Body   *string `json:"body"` // null means a nil body, as Go's fixture runner passes it
}

type testCase struct {
	ID         string          `json:"id"`
	Bidder     string          `json:"bidder"`
	Endpoint   string          `json:"endpoint"`
	ExtraInfo  string          `json:"extra_info"`
	PlatformID string          `json:"platform_id"`
	AppSecret  string          `json:"app_secret"`
	Request    json.RawMessage `json:"request"`
	Response   *response       `json:"response,omitempty"`
	// RequestIndex picks which request MakeBids runs against (a fixture with several http calls
	// has one response per request); default 0.
	RequestIndex int `json:"request_index"`
}

type outRequest struct {
	Method  string              `json:"method"`
	URI     string              `json:"uri"`
	Headers map[string][]string `json:"headers"`
	Body    json.RawMessage     `json:"body"`
	ImpIDs  []string            `json:"imp_ids"`
}

type outBid struct {
	Bid  json.RawMessage `json:"bid"`
	Type string          `json:"type"`
	Seat string          `json:"seat"`
}

type result struct {
	ID          string       `json:"id"`
	BuildError  string       `json:"build_error,omitempty"`
	Panic       string       `json:"panic,omitempty"`
	Requests    []outRequest `json:"requests"`
	Errors      []string     `json:"errors"`
	BidsPresent bool         `json:"bids_present"`
	Currency    string       `json:"currency,omitempty"`
	Bids        []outBid     `json:"bids"`
	BidsErrors  []string     `json:"bids_errors"`
}

// extraRequestInfo mirrors adapterstest.getTestExtraRequestInfo: custom currency rates in
// request.ext.prebid.currency.rates become the conversions; otherwise the info is empty.
func extraRequestInfo(req *openrtb2.BidRequest) *adapters.ExtraRequestInfo {
	var ext struct {
		Prebid *struct {
			Currency *openrtb_ext.ExtRequestCurrency `json:"currency"`
		} `json:"prebid"`
	}
	if len(req.Ext) > 0 && json.Unmarshal(req.Ext, &ext) == nil && ext.Prebid != nil &&
		ext.Prebid.Currency != nil && len(ext.Prebid.Currency.ConversionRates) > 0 &&
		currency.ValidateCustomRates(ext.Prebid.Currency) == nil {
		info := adapters.NewExtraRequestInfo(currency.NewRates(ext.Prebid.Currency.ConversionRates))
		return &info
	}
	return &adapters.ExtraRequestInfo{}
}

func errStrings(errs []error) []string {
	out := make([]string, 0, len(errs))
	for _, e := range errs {
		if e == nil {
			out = append(out, "<nil error>")
		} else {
			out = append(out, e.Error())
		}
	}
	return out
}

func run(c testCase) (res result) {
	res = result{ID: c.ID, Requests: []outRequest{}, Errors: []string{}, Bids: []outBid{}, BidsErrors: []string{}}
	defer func() {
		if r := recover(); r != nil {
			res.Panic = fmt.Sprint(r)
		}
	}()

	builder, ok := builders[c.Bidder]
	if !ok {
		res.BuildError = "unknown bidder " + c.Bidder
		return
	}
	bidder, err := builder(openrtb_ext.BidderName(c.Bidder),
		config.Adapter{Endpoint: c.Endpoint, ExtraAdapterInfo: c.ExtraInfo, PlatformID: c.PlatformID, AppSecret: c.AppSecret},
		config.Server{ExternalUrl: "http://hosturl.com", GvlID: 1, DataCenter: "2"})
	if err != nil {
		res.BuildError = err.Error()
		return
	}

	var req openrtb2.BidRequest
	if err := json.Unmarshal(c.Request, &req); err != nil {
		res.BuildError = "request does not parse as openrtb2.BidRequest: " + err.Error()
		return
	}

	reqs, errs := bidder.MakeRequests(&req, extraRequestInfo(&req))
	res.Errors = errStrings(errs)
	for _, r := range reqs {
		if r == nil {
			res.Errors = append(res.Errors, "<nil request>")
			continue
		}
		headers := map[string][]string{}
		for k, v := range r.Headers {
			headers[k] = v
		}
		body := json.RawMessage(r.Body)
		if !json.Valid(r.Body) {
			body, _ = json.Marshal(string(r.Body))
		}
		if len(r.Body) == 0 {
			body = json.RawMessage("null")
		}
		res.Requests = append(res.Requests, outRequest{Method: r.Method, URI: r.Uri, Headers: headers, Body: body, ImpIDs: r.ImpIDs})
	}

	if c.Response != nil && c.RequestIndex < len(reqs) && reqs[c.RequestIndex] != nil {
		var body []byte
		if c.Response.Body != nil {
			body = []byte(*c.Response.Body)
		}
		br, berrs := bidder.MakeBids(&req, reqs[c.RequestIndex], &adapters.ResponseData{StatusCode: c.Response.Status, Body: body})
		res.BidsErrors = errStrings(berrs)
		if br != nil {
			res.BidsPresent = true
			res.Currency = br.Currency
			for _, b := range br.Bids {
				if b == nil || b.Bid == nil {
					res.BidsErrors = append(res.BidsErrors, "<nil bid>")
					continue
				}
				raw, _ := json.Marshal(b.Bid)
				res.Bids = append(res.Bids, outBid{Bid: raw, Type: string(b.BidType), Seat: string(b.Seat)})
			}
		}
	}
	return
}

func main() {
	in, err := io.ReadAll(os.Stdin)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	var cases []testCase
	if err := json.Unmarshal(in, &cases); err != nil {
		fmt.Fprintln(os.Stderr, "bad input:", err)
		os.Exit(1)
	}
	out := make([]result, 0, len(cases))
	for _, c := range cases {
		out = append(out, run(c))
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(out); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
