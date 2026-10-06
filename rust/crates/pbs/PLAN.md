# pbs: Prebid Server v3.30.0 adapters in Rust

Port of the 238 bidder adapters `rtb-seller-digital` links (`lib/buyers/prebid_builders.go`),
from **Go prebid-server v3.30.0** (the seller's `go.mod` pin), not from this repo's v4 tree:

    /Users/Aryeh.Klein/go/pkg/mod/github.com/prebid/prebid-server/v3@v3.30.0

Goal: mimic the Go library, simpler, with no marshal/unmarshal between models. Adapters take
the typed `ortb::openrtb2::BidRequest` (copied from seller-rs) and read `imp.ext` / `request.ext`
straight from the parsed `Ext`. Stays in this repo until told to move into seller-rs.

## Foundation (done)

| Module | Go source |
|---|---|
| `ortb` (+ `go_json`, `ext_helpers`) | seller-rs `types/ortb` |
| `bidder` | `adapters/bidder.go`, `adapters/response.go` |
| `errortypes` | `errortypes` (codes checked against `code.go`) |
| `bid_types` | `openrtb_ext/bid.go` |
| `header` | `net/http.Header` |
| `currency` | `currency/rates.go`, ISO table from x/text v0.41.0 |
| `jsonutil` | `util/jsonutil` |
| `macros` | `macros` (`{{.Field}}` endpoint templates) |
| `config` | `config.Adapter`, `config.Server` |
| `testing` | `adapters/adapterstest/test_json.go` |

## Status

All 238 adapters are ported from Go v3.30.0, each with its fixtures copied to `tests/fixtures/{name}`
and a `tests/{name}.rs` runner. `cargo test -p pbs`: 316 tests pass, 3 fail
(235 of 238 adapters pass every fixture).

Also built: `registry` (240 bidder names over 238 adapters, from the seller's `prebid_builders.go`)
and `bidder_info` (the seller's 341 `bidder-info/*.yaml`, embedded). `tests/registry.rs` builds every
bidder from its real YAML.

### Known failures (one fixture each; the Rust ORTB model cannot express what Go does)

- **bidmachine, kidoz**: Go tells `Banner.Format == nil` from `len(Format) == 0` (two error texts).
  `Banner.format` is a plain `Vec` read by 47 adapters; retyping it was judged not worth the churn.
- **richaudience**: fixture expects bid type `"no bidtype assigned"`; `BidType` is a `Copy` enum
  (used in 214 files) with `Other` for Go's `""` but no arbitrary string.

### Decisions worth knowing

- The ORTB model was copied from seller-rs, whose `risecodes/openrtb` fork differs from the upstream
  `prebid/openrtb/v20` the Prebid adapters use. Corrected to upstream: pointer scalars (`Video.w/h`,
  `Geo.lat/lon`, `Site.mobile`, ...), `Regs.coppa`, `Native.request` (always written), `Video/Audio.mimes`
  (nil writes `null`), and `TID` / `IP` / `IPv6` / `buyerUid` key aliases. A diff of the two libraries
  found 33 differences; the remaining ones are fields the adapters never touch.
- Buyer responses are parsed strictly (`ortb::de::StrictScope`, used by `jsonutil::unmarshal`), as
  Go's `jsonutil.Unmarshal` is. The seller's own request path stays lenient. Failures inside a bid
  response are reworded like json-iterator (`cannot unmarshal openrtb2.Bid.W: ...`).
- Where Go would panic (empty `seatbid`, `Imp[0]` on no imps) the adapters return an error and say so
  in a comment. Where Go ranges over a map, adapters use first-seen order.
- `audienceNetwork` fails to build from the seller's YAML, as in Go (`disabled: true`, no `platform_id`).

### Not done

- The seller's own wrappers (`pbs_rubicon.go`, `pbs_ogury.go`, `pbs_seedtag.go`, `pbs_triplelift.go`, ...,
  the `Dynamic*` hook maps). They depend on seller types (`SellerContext`, `BuyerConf`) that live in
  seller-rs, so they belong there next to `PbsBuyer`.
- A general case-insensitive JSON key mode (Go matches keys case-insensitively; only four ORTB fields
  have aliases).
- `version.BuildXPrebidHeaderForRequest`; the seller sets the X-Prebid header itself.
- 16 compiler warnings, mostly dead local helper functions left by the porting agents.
- Nothing is committed.

## How to port an adapter

1. Read `adapters/{name}/{name}.go` and `openrtb_ext/imp_{name}.go` in the v3.30.0 tree.
2. Write `src/adapters/{name}.rs`; register it in `src/adapters/mod.rs`.
3. Copy `{name}test/` to `tests/fixtures/{name}/` and add `tests/{name}.rs` (see `tests/cointraffic.rs`).
   Use the endpoint the Go `{name}_test.go` passes to `Builder`.
4. `cargo test -p pbs --test {name}` must pass. Do not edit fixtures to make a test pass.
5. Port the adapter's own `*_test.go` cases that the fixtures do not cover (Builder errors, helpers).

Keep Go's behaviour even where it is awkward (parity is the goal). Where Go would panic, return
an error instead and leave a comment saying so.

## Checklist

| Done | Adapter |
|---|---|
| [x] | 33across |
| [x] | aax |
| [x] | aceex |
| [x] | acuityads |
| [x] | adelement |
| [x] | adf |
| [x] | adgeneration |
| [x] | adhese |
| [x] | adkernel |
| [x] | adkernelAdn |
| [x] | adman |
| [x] | admatic |
| [x] | admixer |
| [x] | adnuntius |
| [x] | adocean |
| [x] | adoppler |
| [x] | adot |
| [x] | adpone |
| [x] | adprime |
| [x] | adquery |
| [x] | adrino |
| [x] | ads_interactive |
| [x] | adsinteractive |
| [x] | adtarget |
| [x] | adtelligent |
| [x] | adtonos |
| [x] | adtrgtme |
| [x] | aduptech |
| [x] | advangelists |
| [x] | adverxo |
| [x] | adview |
| [x] | adxcg |
| [x] | adyoulike |
| [x] | aidem |
| [x] | aja |
| [x] | algorix |
| [x] | alkimi |
| [x] | amx |
| [x] | apacdex |
| [x] | appnexus |
| [x] | appush |
| [x] | aso |
| [x] | audienceNetwork |
| [x] | automatad |
| [x] | avocet |
| [x] | axis |
| [x] | axonix |
| [x] | beachfront |
| [x] | beintoo |
| [x] | bematterfull |
| [x] | between |
| [x] | beyondmedia |
| [ ] | bidmachine |
| [x] | bidmatic |
| [x] | bidmyadz |
| [x] | bidscube |
| [x] | bidstack |
| [x] | bidtheatre |
| [x] | bigoad |
| [x] | blasto |
| [x] | bliink |
| [x] | blue |
| [x] | bluesea |
| [x] | bmtm |
| [x] | boldwin |
| [x] | brave |
| [x] | bwx |
| [x] | cadent_aperture_mx |
| [x] | ccx |
| [x] | cointraffic |
| [x] | coinzilla |
| [x] | colossus |
| [x] | compass |
| [x] | concert |
| [x] | connatix |
| [x] | connectad |
| [x] | consumable |
| [x] | conversant |
| [x] | copper6ssp |
| [x] | cpmstar |
| [x] | criteo |
| [x] | cwire |
| [x] | datablocks |
| [x] | decenterads |
| [x] | deepintent |
| [x] | definemedia |
| [x] | dianomi |
| [x] | displayio |
| [x] | dmx |
| [x] | driftpixel |
| [x] | dxkulture |
| [x] | e_volution |
| [x] | edge226 |
| [x] | emtv |
| [x] | eplanning |
| [x] | epom |
| [x] | escalax |
| [x] | feedad |
| [x] | flipp |
| [x] | freewheelssp |
| [x] | frvradn |
| [x] | gamma |
| [x] | gamoshi |
| [x] | globalsun |
| [x] | gothamads |
| [x] | grid |
| [x] | gumgum |
| [x] | huaweiads |
| [x] | imds |
| [x] | impactify |
| [x] | improvedigital |
| [x] | infytv |
| [x] | inmobi |
| [x] | insticator |
| [x] | interactiveoffers |
| [x] | intertech |
| [x] | invibes |
| [x] | iqx |
| [x] | iqzone |
| [x] | ix |
| [x] | jixie |
| [x] | kargo |
| [x] | kayzen |
| [ ] | kidoz |
| [x] | kiviads |
| [x] | kobler |
| [x] | krushmedia |
| [x] | kueezrtb |
| [x] | lemmadigital |
| [x] | limelightDigital |
| [x] | lm_kiviads |
| [x] | lockerdome |
| [x] | logan |
| [x] | logicad |
| [x] | loopme |
| [x] | loyal |
| [x] | lunamedia |
| [x] | mabidder |
| [x] | madvertise |
| [x] | marsmedia |
| [x] | mediago |
| [x] | medianet |
| [x] | melozen |
| [x] | metax |
| [x] | mgid |
| [x] | mgidX |
| [x] | minutemedia |
| [x] | missena |
| [x] | mobfoxpb |
| [x] | mobilefuse |
| [x] | motorik |
| [x] | nativo |
| [x] | nextmillennium |
| [x] | nobid |
| [x] | ogury |
| [x] | oms |
| [x] | onetag |
| [x] | openweb |
| [x] | openx |
| [x] | operaads |
| [x] | oraki |
| [x] | orbidder |
| [x] | outbrain |
| [x] | ownadx |
| [x] | pangle |
| [x] | pgamssp |
| [x] | playdigo |
| [x] | pubmatic |
| [x] | pubnative |
| [x] | pubrise |
| [x] | pulsepoint |
| [x] | pwbid |
| [x] | qt |
| [x] | readpeak |
| [x] | relevantdigital |
| [x] | resetdigital |
| [x] | revcontent |
| [ ] | richaudience |
| [x] | rise |
| [x] | roulax |
| [x] | rtbhouse |
| [x] | rubicon |
| [x] | sa_lunamedia |
| [x] | screencore |
| [x] | seedingAlliance |
| [x] | seedtag |
| [x] | sharethrough |
| [x] | silvermob |
| [x] | silverpush |
| [x] | smaato |
| [x] | smartadserver |
| [x] | smarthub |
| [x] | smartrtb |
| [x] | smartx |
| [x] | smartyads |
| [x] | smilewanted |
| [x] | smoot |
| [x] | smrtconnect |
| [x] | sonobi |
| [x] | sovrn |
| [x] | sovrnXsp |
| [x] | sspBC |
| [x] | stroeerCore |
| [x] | taboola |
| [x] | tappx |
| [x] | teads |
| [x] | telaria |
| [x] | theadx |
| [x] | thetradedesk |
| [x] | tpmn |
| [x] | tradplus |
| [x] | trafficgate |
| [x] | triplelift |
| [x] | triplelift_native |
| [x] | trustedstack |
| [x] | ucfunnel |
| [x] | undertone |
| [x] | unicorn |
| [x] | unruly |
| [x] | vidazoo |
| [x] | videobyte |
| [x] | videoheroes |
| [x] | vidoomy |
| [x] | visiblemeasures |
| [x] | visx |
| [x] | vox |
| [x] | vrtcal |
| [x] | vungle |
| [x] | xeworks |
| [x] | yahooAds |
| [x] | yandex |
| [x] | yeahmobi |
| [x] | yieldlab |
| [x] | yieldmo |
| [x] | yieldone |
| [x] | zeroclickfraud |
| [x] | zeta_global_ssp |
| [x] | zmaticoo |
