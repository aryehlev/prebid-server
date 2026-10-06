// Code generated from the seller's prebid_builders.go. DO NOT EDIT.

package main

import (
	"github.com/prebid/prebid-server/v3/adapters"
	ttx "github.com/prebid/prebid-server/v3/adapters/33across"
	"github.com/prebid/prebid-server/v3/adapters/aax"
	"github.com/prebid/prebid-server/v3/adapters/aceex"
	"github.com/prebid/prebid-server/v3/adapters/acuityads"
	"github.com/prebid/prebid-server/v3/adapters/adelement"
	"github.com/prebid/prebid-server/v3/adapters/adf"
	"github.com/prebid/prebid-server/v3/adapters/adgeneration"
	"github.com/prebid/prebid-server/v3/adapters/adhese"
	"github.com/prebid/prebid-server/v3/adapters/adkernel"
	"github.com/prebid/prebid-server/v3/adapters/adkernelAdn"
	"github.com/prebid/prebid-server/v3/adapters/adman"
	"github.com/prebid/prebid-server/v3/adapters/admatic"
	"github.com/prebid/prebid-server/v3/adapters/admixer"
	"github.com/prebid/prebid-server/v3/adapters/adnuntius"
	"github.com/prebid/prebid-server/v3/adapters/adocean"
	"github.com/prebid/prebid-server/v3/adapters/adoppler"
	"github.com/prebid/prebid-server/v3/adapters/adot"
	"github.com/prebid/prebid-server/v3/adapters/adpone"
	"github.com/prebid/prebid-server/v3/adapters/adprime"
	"github.com/prebid/prebid-server/v3/adapters/adquery"
	"github.com/prebid/prebid-server/v3/adapters/adrino"
	"github.com/prebid/prebid-server/v3/adapters/ads_interactive"
	"github.com/prebid/prebid-server/v3/adapters/adsinteractive"
	"github.com/prebid/prebid-server/v3/adapters/adtarget"
	"github.com/prebid/prebid-server/v3/adapters/adtelligent"
	"github.com/prebid/prebid-server/v3/adapters/adtonos"
	"github.com/prebid/prebid-server/v3/adapters/adtrgtme"
	"github.com/prebid/prebid-server/v3/adapters/aduptech"
	"github.com/prebid/prebid-server/v3/adapters/advangelists"
	"github.com/prebid/prebid-server/v3/adapters/adverxo"
	"github.com/prebid/prebid-server/v3/adapters/adview"
	"github.com/prebid/prebid-server/v3/adapters/adxcg"
	"github.com/prebid/prebid-server/v3/adapters/adyoulike"
	"github.com/prebid/prebid-server/v3/adapters/aidem"
	"github.com/prebid/prebid-server/v3/adapters/aja"
	"github.com/prebid/prebid-server/v3/adapters/algorix"
	"github.com/prebid/prebid-server/v3/adapters/alkimi"
	"github.com/prebid/prebid-server/v3/adapters/amx"
	"github.com/prebid/prebid-server/v3/adapters/apacdex"
	"github.com/prebid/prebid-server/v3/adapters/appnexus"
	"github.com/prebid/prebid-server/v3/adapters/appush"
	"github.com/prebid/prebid-server/v3/adapters/aso"
	"github.com/prebid/prebid-server/v3/adapters/audienceNetwork"
	"github.com/prebid/prebid-server/v3/adapters/automatad"
	"github.com/prebid/prebid-server/v3/adapters/avocet"
	"github.com/prebid/prebid-server/v3/adapters/axis"
	"github.com/prebid/prebid-server/v3/adapters/axonix"
	"github.com/prebid/prebid-server/v3/adapters/beachfront"
	"github.com/prebid/prebid-server/v3/adapters/beintoo"
	"github.com/prebid/prebid-server/v3/adapters/bematterfull"
	"github.com/prebid/prebid-server/v3/adapters/between"
	"github.com/prebid/prebid-server/v3/adapters/beyondmedia"
	"github.com/prebid/prebid-server/v3/adapters/bidmachine"
	"github.com/prebid/prebid-server/v3/adapters/bidmatic"
	"github.com/prebid/prebid-server/v3/adapters/bidmyadz"
	"github.com/prebid/prebid-server/v3/adapters/bidscube"
	"github.com/prebid/prebid-server/v3/adapters/bidstack"
	"github.com/prebid/prebid-server/v3/adapters/bidtheatre"
	"github.com/prebid/prebid-server/v3/adapters/bigoad"
	"github.com/prebid/prebid-server/v3/adapters/blasto"
	"github.com/prebid/prebid-server/v3/adapters/bliink"
	"github.com/prebid/prebid-server/v3/adapters/blue"
	"github.com/prebid/prebid-server/v3/adapters/bluesea"
	"github.com/prebid/prebid-server/v3/adapters/bmtm"
	"github.com/prebid/prebid-server/v3/adapters/boldwin"
	"github.com/prebid/prebid-server/v3/adapters/brave"
	"github.com/prebid/prebid-server/v3/adapters/bwx"
	cadentaperturemx "github.com/prebid/prebid-server/v3/adapters/cadent_aperture_mx"
	"github.com/prebid/prebid-server/v3/adapters/ccx"
	"github.com/prebid/prebid-server/v3/adapters/cointraffic"
	"github.com/prebid/prebid-server/v3/adapters/coinzilla"
	"github.com/prebid/prebid-server/v3/adapters/colossus"
	"github.com/prebid/prebid-server/v3/adapters/compass"
	"github.com/prebid/prebid-server/v3/adapters/concert"
	"github.com/prebid/prebid-server/v3/adapters/connatix"
	"github.com/prebid/prebid-server/v3/adapters/connectad"
	"github.com/prebid/prebid-server/v3/adapters/consumable"
	"github.com/prebid/prebid-server/v3/adapters/conversant"
	"github.com/prebid/prebid-server/v3/adapters/copper6ssp"
	"github.com/prebid/prebid-server/v3/adapters/cpmstar"
	"github.com/prebid/prebid-server/v3/adapters/criteo"
	"github.com/prebid/prebid-server/v3/adapters/cwire"
	"github.com/prebid/prebid-server/v3/adapters/datablocks"
	"github.com/prebid/prebid-server/v3/adapters/decenterads"
	"github.com/prebid/prebid-server/v3/adapters/deepintent"
	"github.com/prebid/prebid-server/v3/adapters/definemedia"
	"github.com/prebid/prebid-server/v3/adapters/dianomi"
	"github.com/prebid/prebid-server/v3/adapters/displayio"
	"github.com/prebid/prebid-server/v3/adapters/dmx"
	"github.com/prebid/prebid-server/v3/adapters/driftpixel"
	"github.com/prebid/prebid-server/v3/adapters/dxkulture"
	evolution "github.com/prebid/prebid-server/v3/adapters/e_volution"
	"github.com/prebid/prebid-server/v3/adapters/edge226"
	"github.com/prebid/prebid-server/v3/adapters/emtv"
	"github.com/prebid/prebid-server/v3/adapters/eplanning"
	"github.com/prebid/prebid-server/v3/adapters/epom"
	"github.com/prebid/prebid-server/v3/adapters/escalax"
	"github.com/prebid/prebid-server/v3/adapters/feedad"
	"github.com/prebid/prebid-server/v3/adapters/flipp"
	"github.com/prebid/prebid-server/v3/adapters/freewheelssp"
	"github.com/prebid/prebid-server/v3/adapters/frvradn"
	"github.com/prebid/prebid-server/v3/adapters/gamma"
	"github.com/prebid/prebid-server/v3/adapters/gamoshi"
	"github.com/prebid/prebid-server/v3/adapters/globalsun"
	"github.com/prebid/prebid-server/v3/adapters/gothamads"
	"github.com/prebid/prebid-server/v3/adapters/grid"
	"github.com/prebid/prebid-server/v3/adapters/gumgum"
	"github.com/prebid/prebid-server/v3/adapters/huaweiads"
	"github.com/prebid/prebid-server/v3/adapters/imds"
	"github.com/prebid/prebid-server/v3/adapters/impactify"
	"github.com/prebid/prebid-server/v3/adapters/improvedigital"
	"github.com/prebid/prebid-server/v3/adapters/infytv"
	"github.com/prebid/prebid-server/v3/adapters/inmobi"
	"github.com/prebid/prebid-server/v3/adapters/insticator"
	"github.com/prebid/prebid-server/v3/adapters/interactiveoffers"
	"github.com/prebid/prebid-server/v3/adapters/intertech"
	"github.com/prebid/prebid-server/v3/adapters/invibes"
	"github.com/prebid/prebid-server/v3/adapters/iqx"
	"github.com/prebid/prebid-server/v3/adapters/iqzone"
	"github.com/prebid/prebid-server/v3/adapters/ix"
	"github.com/prebid/prebid-server/v3/adapters/jixie"
	"github.com/prebid/prebid-server/v3/adapters/kargo"
	"github.com/prebid/prebid-server/v3/adapters/kayzen"
	"github.com/prebid/prebid-server/v3/adapters/kidoz"
	"github.com/prebid/prebid-server/v3/adapters/kiviads"
	"github.com/prebid/prebid-server/v3/adapters/kobler"
	"github.com/prebid/prebid-server/v3/adapters/krushmedia"
	"github.com/prebid/prebid-server/v3/adapters/kueezrtb"
	"github.com/prebid/prebid-server/v3/adapters/lemmadigital"
	"github.com/prebid/prebid-server/v3/adapters/limelightDigital"
	lmkiviads "github.com/prebid/prebid-server/v3/adapters/lm_kiviads"
	"github.com/prebid/prebid-server/v3/adapters/lockerdome"
	"github.com/prebid/prebid-server/v3/adapters/logan"
	"github.com/prebid/prebid-server/v3/adapters/logicad"
	"github.com/prebid/prebid-server/v3/adapters/loopme"
	"github.com/prebid/prebid-server/v3/adapters/loyal"
	"github.com/prebid/prebid-server/v3/adapters/lunamedia"
	"github.com/prebid/prebid-server/v3/adapters/mabidder"
	"github.com/prebid/prebid-server/v3/adapters/madvertise"
	"github.com/prebid/prebid-server/v3/adapters/marsmedia"
	"github.com/prebid/prebid-server/v3/adapters/mediago"
	"github.com/prebid/prebid-server/v3/adapters/medianet"
	"github.com/prebid/prebid-server/v3/adapters/melozen"
	"github.com/prebid/prebid-server/v3/adapters/metax"
	"github.com/prebid/prebid-server/v3/adapters/mgid"
	"github.com/prebid/prebid-server/v3/adapters/mgidX"
	"github.com/prebid/prebid-server/v3/adapters/minutemedia"
	"github.com/prebid/prebid-server/v3/adapters/missena"
	"github.com/prebid/prebid-server/v3/adapters/mobfoxpb"
	"github.com/prebid/prebid-server/v3/adapters/mobilefuse"
	"github.com/prebid/prebid-server/v3/adapters/motorik"
	"github.com/prebid/prebid-server/v3/adapters/nativo"
	"github.com/prebid/prebid-server/v3/adapters/nextmillennium"
	"github.com/prebid/prebid-server/v3/adapters/nobid"
	"github.com/prebid/prebid-server/v3/adapters/ogury"
	"github.com/prebid/prebid-server/v3/adapters/oms"
	"github.com/prebid/prebid-server/v3/adapters/onetag"
	"github.com/prebid/prebid-server/v3/adapters/openweb"
	"github.com/prebid/prebid-server/v3/adapters/openx"
	"github.com/prebid/prebid-server/v3/adapters/operaads"
	"github.com/prebid/prebid-server/v3/adapters/oraki"
	"github.com/prebid/prebid-server/v3/adapters/orbidder"
	"github.com/prebid/prebid-server/v3/adapters/outbrain"
	"github.com/prebid/prebid-server/v3/adapters/ownadx"
	"github.com/prebid/prebid-server/v3/adapters/pangle"
	"github.com/prebid/prebid-server/v3/adapters/pgamssp"
	"github.com/prebid/prebid-server/v3/adapters/playdigo"
	"github.com/prebid/prebid-server/v3/adapters/pubmatic"
	"github.com/prebid/prebid-server/v3/adapters/pubnative"
	"github.com/prebid/prebid-server/v3/adapters/pubrise"
	"github.com/prebid/prebid-server/v3/adapters/pulsepoint"
	"github.com/prebid/prebid-server/v3/adapters/pwbid"
	"github.com/prebid/prebid-server/v3/adapters/qt"
	"github.com/prebid/prebid-server/v3/adapters/readpeak"
	"github.com/prebid/prebid-server/v3/adapters/relevantdigital"
	"github.com/prebid/prebid-server/v3/adapters/resetdigital"
	"github.com/prebid/prebid-server/v3/adapters/revcontent"
	"github.com/prebid/prebid-server/v3/adapters/richaudience"
	"github.com/prebid/prebid-server/v3/adapters/rise"
	"github.com/prebid/prebid-server/v3/adapters/roulax"
	"github.com/prebid/prebid-server/v3/adapters/rtbhouse"
	"github.com/prebid/prebid-server/v3/adapters/rubicon"
	salunamedia "github.com/prebid/prebid-server/v3/adapters/sa_lunamedia"
	"github.com/prebid/prebid-server/v3/adapters/screencore"
	"github.com/prebid/prebid-server/v3/adapters/seedingAlliance"
	"github.com/prebid/prebid-server/v3/adapters/seedtag"
	"github.com/prebid/prebid-server/v3/adapters/sharethrough"
	"github.com/prebid/prebid-server/v3/adapters/silvermob"
	"github.com/prebid/prebid-server/v3/adapters/silverpush"
	"github.com/prebid/prebid-server/v3/adapters/smaato"
	"github.com/prebid/prebid-server/v3/adapters/smartadserver"
	"github.com/prebid/prebid-server/v3/adapters/smarthub"
	"github.com/prebid/prebid-server/v3/adapters/smartrtb"
	"github.com/prebid/prebid-server/v3/adapters/smartx"
	"github.com/prebid/prebid-server/v3/adapters/smartyads"
	"github.com/prebid/prebid-server/v3/adapters/smilewanted"
	"github.com/prebid/prebid-server/v3/adapters/smoot"
	"github.com/prebid/prebid-server/v3/adapters/smrtconnect"
	"github.com/prebid/prebid-server/v3/adapters/sonobi"
	"github.com/prebid/prebid-server/v3/adapters/sovrn"
	"github.com/prebid/prebid-server/v3/adapters/sovrnXsp"
	"github.com/prebid/prebid-server/v3/adapters/sspBC"
	"github.com/prebid/prebid-server/v3/adapters/stroeerCore"
	"github.com/prebid/prebid-server/v3/adapters/taboola"
	"github.com/prebid/prebid-server/v3/adapters/tappx"
	"github.com/prebid/prebid-server/v3/adapters/teads"
	"github.com/prebid/prebid-server/v3/adapters/telaria"
	"github.com/prebid/prebid-server/v3/adapters/theadx"
	"github.com/prebid/prebid-server/v3/adapters/thetradedesk"
	"github.com/prebid/prebid-server/v3/adapters/tpmn"
	"github.com/prebid/prebid-server/v3/adapters/tradplus"
	"github.com/prebid/prebid-server/v3/adapters/trafficgate"
	"github.com/prebid/prebid-server/v3/adapters/triplelift"
	"github.com/prebid/prebid-server/v3/adapters/triplelift_native"
	"github.com/prebid/prebid-server/v3/adapters/trustedstack"
	"github.com/prebid/prebid-server/v3/adapters/ucfunnel"
	"github.com/prebid/prebid-server/v3/adapters/undertone"
	"github.com/prebid/prebid-server/v3/adapters/unicorn"
	"github.com/prebid/prebid-server/v3/adapters/unruly"
	"github.com/prebid/prebid-server/v3/adapters/vidazoo"
	"github.com/prebid/prebid-server/v3/adapters/videobyte"
	"github.com/prebid/prebid-server/v3/adapters/videoheroes"
	"github.com/prebid/prebid-server/v3/adapters/vidoomy"
	"github.com/prebid/prebid-server/v3/adapters/visiblemeasures"
	"github.com/prebid/prebid-server/v3/adapters/visx"
	"github.com/prebid/prebid-server/v3/adapters/vox"
	"github.com/prebid/prebid-server/v3/adapters/vrtcal"
	"github.com/prebid/prebid-server/v3/adapters/vungle"
	"github.com/prebid/prebid-server/v3/adapters/xeworks"
	"github.com/prebid/prebid-server/v3/adapters/yahooAds"
	"github.com/prebid/prebid-server/v3/adapters/yandex"
	"github.com/prebid/prebid-server/v3/adapters/yeahmobi"
	"github.com/prebid/prebid-server/v3/adapters/yieldlab"
	"github.com/prebid/prebid-server/v3/adapters/yieldmo"
	"github.com/prebid/prebid-server/v3/adapters/yieldone"
	"github.com/prebid/prebid-server/v3/adapters/zeroclickfraud"
	"github.com/prebid/prebid-server/v3/adapters/zeta_global_ssp"
	"github.com/prebid/prebid-server/v3/adapters/zmaticoo"
)

var builders = map[string]adapters.Builder{
	"33across":           ttx.Builder,
	"aax":                aax.Builder,
	"aceex":              aceex.Builder,
	"acuityads":          acuityads.Builder,
	"adelement":          adelement.Builder,
	"adf":                adf.Builder,
	"adgeneration":       adgeneration.Builder,
	"adhese":             adhese.Builder,
	"adkernel":           adkernel.Builder,
	"adkernelAdn":        adkernelAdn.Builder,
	"adman":              adman.Builder,
	"admatic":            admatic.Builder,
	"admixer":            admixer.Builder,
	"adnuntius":          adnuntius.Builder,
	"adocean":            adocean.Builder,
	"adoppler":           adoppler.Builder,
	"adot":               adot.Builder,
	"adpone":             adpone.Builder,
	"adprime":            adprime.Builder,
	"adquery":            adquery.Builder,
	"adrino":             adrino.Builder,
	"ads_interactive":    ads_interactive.Builder,
	"adsinteractive":     adsinteractive.Builder,
	"adtarget":           adtarget.Builder,
	"adtelligent":        adtelligent.Builder,
	"adtonos":            adtonos.Builder,
	"adtrgtme":           adtrgtme.Builder,
	"aduptech":           aduptech.Builder,
	"advangelists":       advangelists.Builder,
	"adverxo":            adverxo.Builder,
	"adview":             adview.Builder,
	"adxcg":              adxcg.Builder,
	"adyoulike":          adyoulike.Builder,
	"aidem":              aidem.Builder,
	"aja":                aja.Builder,
	"algorix":            algorix.Builder,
	"alkimi":             alkimi.Builder,
	"amx":                amx.Builder,
	"apacdex":            apacdex.Builder,
	"appnexus":           appnexus.Builder,
	"appush":             appush.Builder,
	"aso":                aso.Builder,
	"audienceNetwork":    audienceNetwork.Builder,
	"automatad":          automatad.Builder,
	"avocet":             avocet.Builder,
	"axis":               axis.Builder,
	"axonix":             axonix.Builder,
	"beachfront":         beachfront.Builder,
	"beintoo":            beintoo.Builder,
	"bematterfull":       bematterfull.Builder,
	"between":            between.Builder,
	"beyondmedia":        beyondmedia.Builder,
	"bidmachine":         bidmachine.Builder,
	"bidmatic":           bidmatic.Builder,
	"bidmyadz":           bidmyadz.Builder,
	"bidscube":           bidscube.Builder,
	"bidstack":           bidstack.Builder,
	"bidtheatre":         bidtheatre.Builder,
	"bigoad":             bigoad.Builder,
	"blasto":             blasto.Builder,
	"bliink":             bliink.Builder,
	"blue":               blue.Builder,
	"bluesea":            bluesea.Builder,
	"bmtm":               bmtm.Builder,
	"boldwin":            boldwin.Builder,
	"brave":              brave.Builder,
	"bwx":                bwx.Builder,
	"cadent_aperture_mx": cadentaperturemx.Builder,
	"ccx":                ccx.Builder,
	"cointraffic":        cointraffic.Builder,
	"coinzilla":          coinzilla.Builder,
	"colossus":           colossus.Builder,
	"compass":            compass.Builder,
	"concert":            concert.Builder,
	"connatix":           connatix.Builder,
	"connectad":          connectad.Builder,
	"consumable":         consumable.Builder,
	"conversant":         conversant.Builder,
	"copper6ssp":         copper6ssp.Builder,
	"cpmstar":            cpmstar.Builder,
	"criteo":             criteo.Builder,
	"cwire":              cwire.Builder,
	"datablocks":         datablocks.Builder,
	"decenterads":        decenterads.Builder,
	"deepintent":         deepintent.Builder,
	"definemedia":        definemedia.Builder,
	"dianomi":            dianomi.Builder,
	"displayio":          displayio.Builder,
	"dmx":                dmx.Builder,
	"driftpixel":         driftpixel.Builder,
	"dxkulture":          dxkulture.Builder,
	"e_volution":         evolution.Builder,
	"edge226":            edge226.Builder,
	"emtv":               emtv.Builder,
	"emx_digital":        cadentaperturemx.Builder,
	"eplanning":          eplanning.Builder,
	"epom":               epom.Builder,
	"escalax":            escalax.Builder,
	"feedad":             feedad.Builder,
	"flipp":              flipp.Builder,
	"freewheelssp":       freewheelssp.Builder,
	"frvradn":            frvradn.Builder,
	"gamma":              gamma.Builder,
	"gamoshi":            gamoshi.Builder,
	"globalsun":          globalsun.Builder,
	"gothamads":          gothamads.Builder,
	"grid":               grid.Builder,
	"gumgum":             gumgum.Builder,
	"huaweiads":          huaweiads.Builder,
	"imds":               imds.Builder,
	"impactify":          impactify.Builder,
	"improvedigital":     improvedigital.Builder,
	"infytv":             infytv.Builder,
	"inmobi":             inmobi.Builder,
	"insticator":         insticator.Builder,
	"interactiveoffers":  interactiveoffers.Builder,
	"intertech":          intertech.Builder,
	"invibes":            invibes.Builder,
	"iqx":                iqx.Builder,
	"iqzone":             iqzone.Builder,
	"ix":                 ix.Builder,
	"jixie":              jixie.Builder,
	"kargo":              kargo.Builder,
	"kayzen":             kayzen.Builder,
	"kidoz":              kidoz.Builder,
	"kiviads":            kiviads.Builder,
	"kobler":             kobler.Builder,
	"krushmedia":         krushmedia.Builder,
	"kueezrtb":           kueezrtb.Builder,
	"lemmadigital":       lemmadigital.Builder,
	"limelightDigital":   limelightDigital.Builder,
	"lm_kiviads":         lmkiviads.Builder,
	"lockerdome":         lockerdome.Builder,
	"logan":              logan.Builder,
	"logicad":            logicad.Builder,
	"loopme":             loopme.Builder,
	"loyal":              loyal.Builder,
	"lunamedia":          lunamedia.Builder,
	"mabidder":           mabidder.Builder,
	"madvertise":         madvertise.Builder,
	"marsmedia":          marsmedia.Builder,
	"mediafuse":          appnexus.Builder,
	"mediago":            mediago.Builder,
	"medianet":           medianet.Builder,
	"melozen":            melozen.Builder,
	"metax":              metax.Builder,
	"mgid":               mgid.Builder,
	"mgidX":              mgidX.Builder,
	"minutemedia":        minutemedia.Builder,
	"missena":            missena.Builder,
	"mobfoxpb":           mobfoxpb.Builder,
	"mobilefuse":         mobilefuse.Builder,
	"motorik":            motorik.Builder,
	"nativo":             nativo.Builder,
	"nextmillennium":     nextmillennium.Builder,
	"nobid":              nobid.Builder,
	"ogury":              ogury.Builder,
	"oms":                oms.Builder,
	"onetag":             onetag.Builder,
	"openweb":            openweb.Builder,
	"openx":              openx.Builder,
	"operaads":           operaads.Builder,
	"oraki":              oraki.Builder,
	"orbidder":           orbidder.Builder,
	"outbrain":           outbrain.Builder,
	"ownadx":             ownadx.Builder,
	"pangle":             pangle.Builder,
	"pgamssp":            pgamssp.Builder,
	"playdigo":           playdigo.Builder,
	"pubmatic":           pubmatic.Builder,
	"pubnative":          pubnative.Builder,
	"pubrise":            pubrise.Builder,
	"pulsepoint":         pulsepoint.Builder,
	"pwbid":              pwbid.Builder,
	"qt":                 qt.Builder,
	"readpeak":           readpeak.Builder,
	"relevantdigital":    relevantdigital.Builder,
	"resetdigital":       resetdigital.Builder,
	"revcontent":         revcontent.Builder,
	"richaudience":       richaudience.Builder,
	"rise":               rise.Builder,
	"roulax":             roulax.Builder,
	"rtbhouse":           rtbhouse.Builder,
	"rubicon":            rubicon.Builder,
	"sa_lunamedia":       salunamedia.Builder,
	"screencore":         screencore.Builder,
	"seedingAlliance":    seedingAlliance.Builder,
	"seedtag":            seedtag.Builder,
	"sharethrough":       sharethrough.Builder,
	"silvermob":          silvermob.Builder,
	"silverpush":         silverpush.Builder,
	"smaato":             smaato.Builder,
	"smartadserver":      smartadserver.Builder,
	"smarthub":           smarthub.Builder,
	"smartrtb":           smartrtb.Builder,
	"smartx":             smartx.Builder,
	"smartyads":          smartyads.Builder,
	"smilewanted":        smilewanted.Builder,
	"smoot":              smoot.Builder,
	"smrtconnect":        smrtconnect.Builder,
	"sonobi":             sonobi.Builder,
	"sovrn":              sovrn.Builder,
	"sovrnXsp":           sovrnXsp.Builder,
	"sspBC":              sspBC.Builder,
	"stroeerCore":        stroeerCore.Builder,
	"taboola":            taboola.Builder,
	"tappx":              tappx.Builder,
	"teads":              teads.Builder,
	"telaria":            telaria.Builder,
	"theadx":             theadx.Builder,
	"thetradedesk":       thetradedesk.Builder,
	"tpmn":               tpmn.Builder,
	"tradplus":           tradplus.Builder,
	"trafficgate":        trafficgate.Builder,
	"triplelift":         triplelift.Builder,
	"triplelift_native":  triplelift_native.Builder,
	"trustedstack":       trustedstack.Builder,
	"ucfunnel":           ucfunnel.Builder,
	"undertone":          undertone.Builder,
	"unicorn":            unicorn.Builder,
	"unruly":             unruly.Builder,
	"vidazoo":            vidazoo.Builder,
	"videobyte":          videobyte.Builder,
	"videoheroes":        videoheroes.Builder,
	"vidoomy":            vidoomy.Builder,
	"visiblemeasures":    visiblemeasures.Builder,
	"visx":               visx.Builder,
	"vox":                vox.Builder,
	"vrtcal":             vrtcal.Builder,
	"vungle":             vungle.Builder,
	"xeworks":            xeworks.Builder,
	"yahooAds":           yahooAds.Builder,
	"yandex":             yandex.Builder,
	"yeahmobi":           yeahmobi.Builder,
	"yieldlab":           yieldlab.Builder,
	"yieldmo":            yieldmo.Builder,
	"yieldone":           yieldone.Builder,
	"zeroclickfraud":     zeroclickfraud.Builder,
	"zeta_global_ssp":    zeta_global_ssp.Builder,
	"zmaticoo":           zmaticoo.Builder,
}
