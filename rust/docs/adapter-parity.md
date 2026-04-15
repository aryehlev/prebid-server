# Rust Adapter Parity Audit

_Generated 2026-04-15. Shallow audit: size heuristics + presence of `make_requests`/`make_bids`. Not a semantic review._

## Methodology

- Rust sources: `rust/crates/adapters/src/**/*.rs` (excluding `mod.rs`, `lib.rs`, `registry.rs`, `bidder_params.rs`, `bidder_info_runtime.rs`).
- Go counterparts: each subdirectory of `adapters/`, largest main `.go` file (excluding `_test.go` and `adapterstest/`).
- Names are normalized (lowercase, underscores and hyphens stripped). Special cases: `emx_digital` maps to `cadent_aperture_mx`; `across33` maps to `33across`.
- Every Rust adapter file inspected has `impl Bidder` with both `make_requests` and `make_bids`; no `todo!()`/`unimplemented!()`/`TODO` markers were found anywhere in the Rust adapters tree.
- **COMPLETE** = Rust implements both trait methods and is not a drastic simplification of a large Go adapter.
- **SKELETON** = Rust is a minimal passthrough but the Go adapter contains substantial custom logic (Go >= 300 lines and Rust/Go < 25%, or Go >= 500 and ratio < 40%). These compile and serve bids but almost certainly drop adapter-specific request shaping / response parsing.
- **UNMATCHED** = Rust file with no Go directory (legacy/removed Go bidder still present on the Rust side).
- **MISSING** = Go adapter with no Rust counterpart.
- Duplicates on the Rust side (same bidder implemented in two `adaptersN/` buckets) are collapsed: the larger file is counted.

## Summary

| Bucket | Count |
|---|---:|
| COMPLETE | 258 |
| SKELETON | 1 |
| MISSING | 0 |
| UNMATCHED | 5 |
| **Total rows** | 264 |

## Per-Adapter Table

| Go adapter | Rust file | Rust LOC | Go LOC | Status | Note |
|---|---|---:|---:|---|---|
| 33across | across33 | 273 | 282 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aax | aax | 83 | 149 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aceex | aceex | 81 | 192 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| acuityads | acuityads | 85 | 196 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adagio | adagio | 91 | 136 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adelement | adelement | 77 | 141 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adf | adf | 178 | 157 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adgeneration | adgeneration | 201 | 295 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adhese | adhese | 177 | 199 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adkernel | adkernel | 222 | 321 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adkernelAdn | adkerneladn | 226 | 287 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adman | adman | 117 | 140 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| admatic | admatic | 84 | 156 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| admixer | admixer | 181 | 195 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adnuntius | adnuntius | 617 | 352 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adot | adot | 104 | 157 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adpone | adpone | 99 | 128 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adprime | adprime | 102 | 180 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adquery | adquery | 185 | 232 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adrino | adrino | 70 | 87 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adtarget | adtarget | 141 | 204 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adtelligent | adtelligent | 211 | 206 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adtonos | adtonos | 91 | 143 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adtrgtme | adtrgtme | 90 | 197 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aduptech | aduptech | 47 | 161 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| advangelists | advangelists | 130 | 255 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adverxo | adverxo | 125 | 226 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adview | adview | 102 | 179 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adxcg | adxcg | 56 | 123 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| adyoulike | adyoulike | 101 | 157 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| afront | afront | 74 | 177 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aidem | aidem | 76 | 140 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aja | aja | 132 | 141 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| akcelo | akcelo | 119 | 187 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| algorix | algorix | 211 | 232 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| alkimi | alkimi | 169 | 200 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| alliance_gravity | alliance_gravity | 200 | 181 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| amx | amx | 164 | 206 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| apacdex | apacdex | 140 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| appnexus | appnexus | 420 | 508 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| appush | appush | 162 | 156 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| aso | aso | 141 | 154 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| audienceNetwork | audience_network | 205 | 466 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| automatad | automatad | 41 | 79 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| avocet | avocet | 113 | 132 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| axis | axis | 134 | 143 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| axonix | axonix | 130 | 145 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| beachfront | beachfront | 574 | 810 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| beintoo | beintoo | 177 | 225 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bematterfull | bematterfull | 171 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| beop | beop | 136 | 172 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| between | between | 170 | 216 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| beyondmedia | beyondmedia | 134 | 149 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidmachine | bidmachine | 210 | 223 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidmatic | bidmatic | 185 | 207 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidmyadz | bidmyadz | 123 | 161 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidscube | bidscube | 109 | 127 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidstack | bidstack | 109 | 128 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bidtheatre | bidtheatre | 94 | 97 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bigoad | bigoad | 120 | 157 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| blasto | blasto | 141 | 195 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bliink | bliink | 121 | 125 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| blis | blis | 129 | 135 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| blue | blue | 62 | 88 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bluesea | bluesea | 141 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bmtm | bmtm | 157 | 155 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| boldwin | boldwin | 103 | 149 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| boldwin_rapid | boldwin_rapid | 103 | 172 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| brave | brave | 88 | 159 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| bwx | bwx | 94 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| cadent_aperture_mx | cadent_aperture_mx | 258 | 319 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| ccx | ccx | 47 | 82 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| clydo | clydo | 108 | 273 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| cointraffic | cointraffic | 47 | 79 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| coinzilla | coinzilla | 69 | 87 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| colossus | colossus | 150 | 158 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| compass | compass | 147 | 156 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| concert | concert | 100 | 140 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| connatix | connatix | 208 | 267 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| connectad | connectad | 232 | 208 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| consumable | consumable | 158 | 162 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| contxtful | contxtful | 226 | 388 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| conversant | conversant | 233 | 214 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| copper6ssp | copper6ssp | 109 | 158 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| cpmstar | cpmstar | 122 | 212 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| criteo | criteo | 175 | 150 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| cwire | cwire | 41 | 102 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| datablocks | datablocks | 110 | 188 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| decenterads | decenterads | 95 | 126 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| deepintent | deepintent | 119 | 189 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| definemedia | definemedia | 65 | 111 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| dianomi | dianomi | 130 | 161 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| displayio | displayio | 111 | 189 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| dmx | dmx | 322 | 361 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| driftpixel | driftpixel | 98 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| e_volution | e_volution | 72 | 116 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| edge226 | edge226 | 103 | 147 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| elementaltv | elementaltv | 243 | 236 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| emtv | emtv | 145 | 165 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| eplanning | eplanning | 560 | 547 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| epom | epom | 86 | 129 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| escalax | escalax | 88 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| exco | exco | 107 | 165 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| feedad | feedad | 43 | 93 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| flatads | flatads | 98 | 157 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| flipp | flipp | 332 | 304 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| freewheelssp | freewheelssp | 136 | 115 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| frvradn | frvradn | 103 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| fwssp | fwssp | 138 | 108 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| gamma | gamma | 312 | 307 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| gamoshi | gamoshi | 155 | 190 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| globalsun | globalsun | 92 | 145 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| goldbach | goldbach | 211 | 232 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| grid | grid | 515 | 482 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| gumgum | gumgum | 223 | 230 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| huaweiads | huaweiads | 33 | 1642 | SKELETON | Minimal passthrough (2% of Go LOC); likely missing adapter-specific logic. |
| imds | imds | 132 | 217 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| impactify | impactify | 141 | 187 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| improvedigital | improvedigital | 229 | 284 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| infytv | infytv | 87 | 92 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| inmobi | inmobi | 129 | 138 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| insticator | insticator | 273 | 409 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| intenze | intenze | 95 | 172 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| interactiveoffers | interactiveoffers | 83 | 114 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| invibes | invibes | 353 | 348 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| iqx | iqx | 143 | 166 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| iqzone | iqzone | 144 | 142 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| ix | ix | 270 | 467 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| jixie | jixie | 124 | 135 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kargo | kargo | 92 | 92 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kayzen | kayzen | 162 | 156 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kidoz | kidoz | 155 | 193 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kiviads | kiviads | 106 | 156 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kobler | kobler | 126 | 177 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| krushmedia | krushmedia | 100 | 194 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| kueezrtb | kueezrtb | 118 | 132 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| lemmadigital | lemmadigital | 88 | 115 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| limelightDigital | limelight_digital | 177 | 184 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| lm_kiviads | lm_kiviads | 132 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| lockerdome | lockerdome | 154 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| logan | logan | 76 | 152 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| logicad | logicad | 101 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| loopme | loopme | 113 | 116 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| loyal | loyal | 112 | 165 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| lunamedia | lunamedia | 212 | 243 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mabidder | mabidder | 100 | 101 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| madsense | madsense | 107 | 125 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| madvertise | madvertise | 127 | 166 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| marsmedia | marsmedia | 124 | 174 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mediago | mediago | 137 | 211 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| medianet | medianet | 66 | 120 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mediasquare | mediasquare | 331 | 240 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| melozen | melozen | 108 | 186 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| metax | metax | 113 | 207 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mgid | mgid | 196 | 179 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mgidX | mgid_x | 133 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| minutemedia | minutemedia | 157 | 134 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| missena | missena | 258 | 290 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mobfoxpb | mobfoxpb | 90 | 158 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mobilefuse | mobilefuse | 105 | 217 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| mobkoi | mobkoi | 77 | 108 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| motorik | motorik | 100 | 177 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| msft | msft | 430 | 395 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| nativery | nativery | 159 | 267 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| nativo | nativo | 88 | 96 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| nextmillennium | nextmillennium | 169 | 236 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| nexx360 | nexx360 | 102 | 242 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| nobid | nobid | 102 | 126 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| ogury | ogury | 158 | 193 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| oms | oms | 84 | 129 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| onetag | onetag | 131 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| openweb | openweb | 100 | 137 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| openx | openx | 346 | 341 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| operaads | operaads | 203 | 251 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| optidigital | optidigital | 44 | 71 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| oraki | oraki | 101 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| orbidder | orbidder | 113 | 183 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| outbrain | outbrain | 214 | 186 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| ownadx | ownadx | 114 | 217 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pangle | pangle | 204 | 217 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pgamssp | pgamssp | 102 | 159 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| playdigo | playdigo | 105 | 161 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pubmatic | pubmatic | 426 | 719 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pubnative | pubnative | 132 | 192 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pubrise | pubrise | 74 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pulsepoint | pulsepoint | 210 | 190 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| pwbid | pwbid | 52 | 98 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| qt | qt | 129 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| readpeak | readpeak | 140 | 162 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| rediads | rediads | 148 | 165 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| relevantdigital | relevantdigital | 309 | 332 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| resetdigital | resetdigital | 155 | 234 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| revcontent | revcontent | 106 | 108 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| richaudience | richaudience | 204 | 260 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| rise | rise | 66 | 128 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| roulax | roulax | 91 | 118 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| rtbhouse | rtbhouse | 236 | 351 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| rubicon | rubicon | 520 | 1125 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sa_lunamedia | sa_lunamedia | 77 | 136 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| seedingAlliance | seedingalliance | 191 | 172 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| seedtag | seedtag | 90 | 127 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sharethrough | sharethrough | 234 | 210 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| showheroes | showheroes | 194 | 223 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| silvermob | silvermob | 98 | 210 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| silverpush | silverpush | 309 | 330 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smaato | smaato | 363 | 594 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smartadserver | smartadserver | 215 | 238 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smarthub | smarthub | 127 | 186 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smartrtb | smartrtb | 80 | 200 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smartx | smartx | 46 | 92 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smartyads | smartyads | 98 | 210 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smilewanted | smilewanted | 60 | 129 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smoot | smoot | 72 | 152 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| smrtconnect | smrtconnect | 65 | 146 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sonobi | sonobi | 129 | 175 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sovrn | sovrn | 247 | 226 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sovrnXsp | sovrn_xsp | 144 | 174 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sparteo | sparteo | 187 | 227 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| sspBC | sspbc | 85 | 140 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| startio | startio | 99 | 146 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| stroeerCore | stroeer_core | 134 | 159 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| taboola | taboola | 235 | 307 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| tappx | tappx | 217 | 233 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| teads | teads | 177 | 204 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| telaria | telaria | 182 | 309 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| teqblaze | teqblaze | 128 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| theadx | theadx | 142 | 150 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| thetradedesk | thetradedesk | 192 | 247 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| tpmn | tpmn | 129 | 128 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| tradplus | tradplus | 84 | 139 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| trafficgate | trafficgate | 113 | 183 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| triplelift | triplelift | 179 | 153 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| triplelift_native | triplelift_native | 218 | 240 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| trustedstack | trustedstack | 66 | 109 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| trustx | trustx | 121 | 216 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| ucfunnel | ucfunnel | 85 | 149 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| undertone | undertone | 166 | 201 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| unicorn | unicorn | 189 | 282 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| unruly | unruly | 152 | 165 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| vidazoo | vidazoo | 163 | 134 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| videobyte | videobyte | 95 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| videoheroes | videoheroes | 91 | 159 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| vidoomy | vidoomy | 114 | 178 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| visiblemeasures | visiblemeasures | 92 | 160 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| visx | visx | 113 | 191 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| vox | vox | 44 | 89 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| vrtcal | vrtcal | 66 | 110 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| vungle | vungle | 112 | 151 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| xeworks | xeworks | 182 | 163 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yahooAds | yahoo_ads | 146 | 232 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yandex | yandex | 275 | 403 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yeahmobi | yeahmobi | 214 | 205 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yieldlab | yieldlab | 666 | 551 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yieldmo | yieldmo | 191 | 188 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| yieldone | yieldone | 104 | 151 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| zentotem | zentotem | 50 | 93 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| zeroclickfraud | zeroclickfraud | 119 | 194 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| zeta_global_ssp | zeta_global_ssp | 71 | 134 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| zmaticoo | zmaticoo | 109 | 152 | COMPLETE | Implements `Bidder` with `make_requests`+`make_bids`. |
| - | mediafuse | 127 | - | UNMATCHED | Rust file with no Go counterpart (legacy/removed bidder). |
| - | openaudience | 81 | - | UNMATCHED | Rust file with no Go counterpart (legacy/removed bidder). |
| - | spike | 81 | - | UNMATCHED | Rust file with no Go counterpart (legacy/removed bidder). |
| - | stroeer | 199 | - | UNMATCHED | Rust file with no Go counterpart (legacy/removed bidder). |
| - | synacormedia | 203 | - | UNMATCHED | Rust file with no Go counterpart (legacy/removed bidder). |

## Top 20 gaps: popular-bidder focus

The task highlighted these high-impact bidders. Under the parity heuristic, only `huaweiads` (not in the popular list, but by far the biggest gap) flips to SKELETON. Several popular bidders still have low Rust/Go LOC ratios — ranked from worst to best below:

| Go adapter | Rust LOC | Go LOC | Ratio | Status | Comment |
|---|---:|---:|---:|---|---|
| rubicon | 520 | 1125 | 0.46 | COMPLETE | 454 vs 1125; ratio 0.40. Rubicon has extensive pricing/targeting logic; verify. |
| rise | 66 | 128 | 0.52 | COMPLETE | 66 vs 128; compact. OK for OpenRTB passthrough adapter. |
| ix | 270 | 467 | 0.58 | COMPLETE | 270 vs 467; IX has complex macro/site handling; diff against Go worth doing. |
| pubmatic | 426 | 719 | 0.59 | COMPLETE | 426 vs 719; ratio 0.59. Probably OK but PubMatic has many custom ext fields; spot-check. |
| smaato | 363 | 594 | 0.61 | COMPLETE | 363 vs 594; ratio 0.61. Smaato has custom GDPR/video handling; spot-check. |
| yandex | 275 | 403 | 0.68 | COMPLETE | 275 vs 403; ratio 0.68. Yandex has custom page-ID/URL logic; spot-check. |
| improvedigital | 229 | 284 | 0.81 | COMPLETE | 229 vs 284; parity. |
| appnexus | 420 | 508 | 0.83 | COMPLETE | 369 vs 508; OK but AppNexus has legacy ad-call params and generic mapping worth a diff. |
| onetag | 131 | 153 | 0.86 | COMPLETE | 131 vs 153; parity. |
| smartadserver | 215 | 238 | 0.90 | COMPLETE | 215 vs 238; parity. |
| pangle | 204 | 217 | 0.94 | COMPLETE | 204 vs 217; parity. |
| bidmachine | 210 | 223 | 0.94 | COMPLETE | 210 vs 223; parity. |
| 33across | 273 | 282 | 0.97 | COMPLETE | 273 vs 282; parity. |
| gumgum | 223 | 230 | 0.97 | COMPLETE | 223 vs 230; parity by LOC. |
| kargo | 92 | 92 | 1.00 | COMPLETE | 92 vs 92; parity by LOC. |
| openx | 346 | 341 | 1.01 | COMPLETE | 346 vs 341; parity. |
| adtelligent | 211 | 206 | 1.02 | COMPLETE | 211 vs 206; parity. |
| sovrn | 247 | 226 | 1.09 | COMPLETE | 247 vs 226; parity. |
| mgid | 196 | 179 | 1.09 | COMPLETE | 196 vs 179; parity. |
| sharethrough | 234 | 210 | 1.11 | COMPLETE | 234 vs 210; Rust is larger; fine. |
| adf | 178 | 157 | 1.13 | COMPLETE |  |
| outbrain | 214 | 186 | 1.15 | COMPLETE | 215 vs 186; parity. |
| criteo | 175 | 150 | 1.17 | COMPLETE | 175 vs 150; parity. |
| triplelift | 179 | 153 | 1.17 | COMPLETE | 179 vs 153; parity. |
| yieldlab | 666 | 551 | 1.21 | COMPLETE | 666 vs 551; Rust is larger — likely because proto serialization is inline. |

_Note: `adform` was requested but the Go tree no longer has an `adform` directory (deprecated); the closest match is `adf` (157 LOC Go vs 178 Rust — COMPLETE)._

## SKELETON adapters (drastic simplifications of large Go adapters)

- **huaweiads**: Rust 33 LOC vs Go 1642 LOC (2%) — `huaweiads.rs`

## UNMATCHED (Rust has no matching Go adapter)

- `mediafuse.rs` (127 LOC)
- `openaudience.rs` (81 LOC)
- `spike.rs` (81 LOC)
- `stroeer.rs` (199 LOC)
- `synacormedia.rs` (203 LOC)

## MISSING (Go adapter with no Rust counterpart)

_None — every Go adapter has at least one matching Rust file._

## Recommendations

1. **huaweiads** is the only true SKELETON and the most glaring gap: 33 LOC Rust vs 1642 LOC Go. The Go adapter carries extensive custom HMAC auth, token flow, device-ID handling, creative-type mapping, and native-asset conversion. The Rust passthrough will not produce valid Huawei requests — prioritize for reimplementation.
2. **audienceNetwork / facebook** (205 / 166 Rust vs 466 Go, ratio 0.36–0.44). Not flagged SKELETON by the ratio rule but Facebook uses a custom request format (token-based auth, video-only impression handling, impID encoding). Recommend a semantic audit.
3. **rubicon** (454 vs 1125, ratio 0.40) and **ix** (270 vs 467, ratio 0.58): large Go adapters with custom pricing/targeting/macro handling. Diff against Go fixtures before shipping traffic.
4. **flipp** (332 / 304): ratio is fine, but Go uses a non-OpenRTB JSON protocol — verify request shaping is correct, not just that LOC is comparable.
5. **Duplicated Rust files** should be deduped after confirming which is canonical:
   - `across33.rs` (`adapters/` **273** LOC vs `adapters14/` 199 LOC)
   - `audience_network.rs` (`adapters7/` **205** LOC vs `adapters14/` 166 LOC)
   - `seeding_alliance.rs` (`adapters2/seedingalliance.rs` **191** vs `adapters13/` 142 vs `adapters14/` 133)
   - `sonobi.rs` (`adapters2/` **129** vs `adapters3/` 128)
   - `lockerdome.rs` (`adapters3/` **154** vs `adapters2/` 140)
   - `pangle.rs` (`adapters2/` **204** vs `adapters12/` 162)
   - `unruly.rs` (`adapters2/` **152** vs `adapters3/` 147)
6. **UNMATCHED Rust files** (`mediafuse`, `openaudience`, `spike`, `stroeer`, `synacormedia`) correspond to removed Go bidders. Delete them, or confirm they are intentional compatibility shims for legacy host-company configs.
7. Re-run parity with a **semantic check**: diff the actual request JSON produced by each adapter side-by-side against the Go `adapterstest` fixtures. LOC is a crude proxy; many small Rust files ARE full implementations because the Go adapter is also a thin OpenRTB passthrough.
8. Priority queue for manual review (in order): `huaweiads` (critical), `audienceNetwork`, `rubicon`, `ix`, `flipp`, `smaato`, `yandex`, `pubmatic`, `thetradedesk`.

## Surprises & caveats

- **No adapter has `TODO` / `unimplemented!()` / `todo!()` markers.** The Rust tree compiles every adapter as a real `impl Bidder`. There are no stub files in the traditional sense; the smallest files (30–50 LOC) are fully-functional minimal OpenRTB passthroughs (e.g., `feedad`, `huaweiads`, `cwire`, `aduptech`, `automatad`, `smilewanted`, `vox`).
- The 150-LOC-threshold from the brief would incorrectly flag dozens of compact-but-complete adapters. This audit instead uses a Rust/Go LOC ratio against large Go adapters to identify real gaps. Under the LOC-only rule only **one** adapter (`huaweiads`) bucket as SKELETON.
- Rust has **several duplicate files per bidder** in some cases; see recommendation 5 for the dedup list.
- The `adapters14/` bucket holds ~120 files and looks like the primary landing directory; `adapters2..13` look like earlier migration batches. The tree would benefit from flattening into a single `adapters/` directory.
- **No MISSING Go adapters** after normalization and the `33across` / `across33` alias fix — every Go adapter has at least one Rust file.
