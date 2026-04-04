use std::collections::HashMap;
use crate::Bidder;

fn read_endpoint(name: &str, default: &str) -> String {
    let paths = [
        format!("/home/user/prebid-server/static/bidder-info/{}.yaml", name),
        format!("static/bidder-info/{}.yaml", name),
        format!("../static/bidder-info/{}.yaml", name),
    ];
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("endpoint:") && !line.starts_with('#') {
                    let val = line.trim_start_matches("endpoint:").trim().trim_matches('"');
                    if !val.is_empty() {
                        return val.to_string();
                    }
                }
            }
        }
    }
    default.to_string()
}

fn ep(name: &str) -> String { read_endpoint(name, "") }

struct GenericAdapter { endpoint: String }
impl GenericAdapter {
    fn boxed(endpoint: String) -> Box<dyn Bidder> { Box::new(Self { endpoint }) }
}
impl Bidder for GenericAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &crate::ExtraRequestInfo) -> (Vec<crate::RequestData>, Vec<crate::BidderError>) {
        if self.endpoint.is_empty() { return (vec![], vec![]); }
        match serde_json::to_vec(request) {
            Ok(body) => (vec![crate::RequestData::new_post(&self.endpoint, body)], vec![]),
            Err(e) => (vec![], vec![crate::BidderError::FailedToRequestBids(e.to_string())]),
        }
    }
    fn make_bids(&self, _: &openrtb::BidRequest, _: &crate::RequestData, response: &crate::ResponseData) -> Result<crate::BidderResponse, Vec<crate::BidderError>> {
        if response.status_code == 204 { return Ok(crate::BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![crate::BidderError::BadServerResponse(e.to_string())])?;
        let mut result = crate::BidderResponse::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = crate::get_bid_type_from_mtype(bid.mtype.unwrap_or(0));
                result.bids.push(crate::TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

pub fn build_adapter_map() -> HashMap<String, Box<dyn Bidder>> {
    let mut m: HashMap<String, Box<dyn Bidder>> = HashMap::new();
    macro_rules! reg {
        ($key:expr, $adapter:expr) => { m.insert($key.to_string(), Box::new($adapter)); };
    }
    macro_rules! gen {
        ($key:expr) => { m.insert($key.to_string(), GenericAdapter::boxed(ep($key))); };
    }

    // adapters/ — real implementations
    reg!("appnexus", crate::adapters::AppnexusAdapter::new(ep("appnexus")));
    reg!("sovrn", crate::adapters::SovrnAdapter::new(ep("sovrn")));
    reg!("rubicon", crate::adapters::RubiconAdapter::new(ep("rubicon"), String::new(), String::new()));
    reg!("ix", crate::adapters::IxAdapter::new(ep("ix")));
    reg!("openx", crate::adapters::OpenxAdapter::new(ep("openx"), "openx".to_string()));
    reg!("pubmatic", crate::adapters::PubmaticAdapter::new(ep("pubmatic")));
    reg!("33across", crate::adapters::Across33Adapter::new(ep("33across")));
    reg!("beachfront", crate::adapters::BeachfrontAdapter::new(ep("beachfront")));
    reg!("criteo", crate::adapters::CriteoAdapter::new(ep("criteo")));
    reg!("sharethrough", crate::adapters::SharethroughAdapter::new(ep("sharethrough")));

    // adapters2/
    reg!("smartadserver", crate::adapters2::SmartadserverAdapter::new(ep("smartadserver")));
    reg!("triplelift", crate::adapters2::TripleliftAdapter::new(ep("triplelift")));
    reg!("taboola", crate::adapters2::TaboolaAdapter::new(ep("taboola")));
    reg!("outbrain", crate::adapters2::OutbrainAdapter::new(ep("outbrain")));
    reg!("grid", crate::adapters2::GridAdapter::new(ep("grid")));
    reg!("smaato", crate::adapters2::SmaatoAdapter::new(ep("smaato")));
    reg!("richaudience", crate::adapters2::RichaudienceAdapter::new(ep("richaudience")));
    reg!("pulsepoint", crate::adapters2::PulsepointAdapter::new(ep("pulsepoint")));
    reg!("rtbhouse", crate::adapters2::RtbhouseAdapter::new(ep("rtbhouse")));
    reg!("conversant", crate::adapters2::ConversantAdapter::new(ep("conversant")));
    reg!("emx_digital", crate::adapters2::EmxDigitalAdapter::new(ep("emx_digital")));
    reg!("yandex", crate::adapters2::YandexAdapter::new(ep("yandex")));
    reg!("onetag", crate::adapters2::OnetagAdapter::new(ep("onetag")));
    reg!("inmobi", crate::adapters2::InmobiAdapter::new(ep("inmobi")));
    reg!("gumgum", crate::adapters2::GumgumAdapter::new(ep("gumgum")));
    reg!("lockerdome", crate::adapters2::LockerdomeAdapter::new(ep("lockerdome")));
    reg!("openaudience", crate::adapters2::OpenaudienceAdapter::new(ep("openaudience")));
    reg!("pangle", crate::adapters2::PangleAdapter::new(ep("pangle")));
    reg!("seedingalliance", crate::adapters2::SeedingallianceAdapter::new(ep("seedingalliance")));
    reg!("sonobi", crate::adapters2::SonobiAdapter::new(ep("sonobi")));
    reg!("spike", crate::adapters2::SpikeAdapter::new(ep("spike")));
    reg!("stroeer", crate::adapters2::StroeerAdapter::new(ep("stroeer")));
    reg!("synacormedia", crate::adapters2::SynacormediaAdapter::new(ep("synacormedia")));
    reg!("unruly", crate::adapters2::UnrulyAdapter::new(ep("unruly")));

    // adapters3/
    reg!("teads", crate::adapters3::TeadsAdapter::new(ep("teads")));
    reg!("revcontent", crate::adapters3::RevcontentAdapter::new(ep("revcontent")));
    reg!("adtelligent", crate::adapters3::AdtelligentAdapter::new(ep("adtelligent")));
    reg!("adkernel", crate::adapters3::AdkernelAdapter::new(ep("adkernel")));
    reg!("admixer", crate::adapters3::AdmixerAdapter::new(ep("admixer")));
    reg!("adman", crate::adapters3::AdmanAdapter::new(ep("adman")));
    reg!("nativo", crate::adapters3::NativoAdapter::new(ep("nativo")));
    reg!("nobid", crate::adapters3::NobidAdapter::new(ep("nobid")));
    reg!("eplanning", crate::adapters3::EplanningAdapter::new(ep("eplanning")));
    reg!("loopme", crate::adapters3::LoopmeAdapter::new(ep("loopme")));
    reg!("vidazoo", crate::adapters3::VidazooAdapter::new(ep("vidazoo")));
    reg!("triplelift_native", crate::adapters3::TripleliftNativeAdapter::new(ep("triplelift_native")));

    // adapters4/
    reg!("colossus", crate::adapters4::ColossusAdapter::new(ep("colossus")));
    reg!("connectad", crate::adapters4::ConnectadAdapter::new(ep("connectad")));
    reg!("consumable", crate::adapters4::ConsumableAdapter::new(ep("consumable")));
    reg!("datablocks", crate::adapters4::DatablocksAdapter::new(ep("datablocks")));
    reg!("deepintent", crate::adapters4::DeepintentAdapter::new(ep("deepintent")));
    reg!("dmx", crate::adapters4::DmxAdapter::new(ep("dmx")));
    reg!("emtv", crate::adapters4::EmtvAdapter::new(ep("emtv")));
    reg!("epom", crate::adapters4::EpomAdapter::new(ep("epom")));
    reg!("gamma", crate::adapters4::GammaAdapter::new(ep("gamma")));
    reg!("gamoshi", crate::adapters4::GamoshiAdapter::new(ep("gamoshi")));
    reg!("goldbach", crate::adapters4::GoldbachAdapter::new(ep("goldbach")));
    reg!("improvedigital", crate::adapters4::ImprovedigitalAdapter::new(ep("improvedigital")));
    reg!("infytv", crate::adapters4::InfytvAdapter::new(ep("infytv")));
    reg!("insticator", crate::adapters4::InsticatorAdapter::new(ep("insticator")));
    reg!("kargo", crate::adapters4::KargoAdapter::new(ep("kargo")));

    // adapters5/
    reg!("jixie", crate::adapters5::JixieAdapter::new(ep("jixie")));
    reg!("kayzen", crate::adapters5::KayzenAdapter::new(ep("kayzen")));
    reg!("kidoz", crate::adapters5::KidozAdapter::new(ep("kidoz")));
    reg!("kobler", crate::adapters5::KoblerAdapter::new(ep("kobler")));
    reg!("lunamedia", crate::adapters5::LunamediaAdapter::new(ep("lunamedia")));
    reg!("madvertise", crate::adapters5::MadvertiseAdapter::new(ep("madvertise")));
    reg!("medianet", crate::adapters5::MedianetAdapter::new(ep("medianet")));
    reg!("mgid", crate::adapters5::MgidAdapter::new(ep("mgid")));
    reg!("minutemedia", crate::adapters5::MinutemediaAdapter::new(ep("minutemedia")));

    // adapters6/
    reg!("aax", crate::adapters6::AaxAdapter::new(ep("aax")));
    reg!("aceex", crate::adapters6::AceexAdapter::new(ep("aceex")));
    reg!("acuityads", crate::adapters6::AcuityadsAdapter::new(ep("acuityads")));
    reg!("adagio", crate::adapters6::AdagioAdapter::new(ep("adagio")));
    reg!("adelement", crate::adapters6::AdelementAdapter::new(ep("adelement")));
    reg!("adf", crate::adapters6::AdfAdapter::new(ep("adf")));
    reg!("adgeneration", crate::adapters6::AdgenerationAdapter::new(ep("adgeneration")));
    reg!("adhese", crate::adapters6::AdheseAdapter::new(ep("adhese")));
    reg!("adkerneladn", crate::adapters6::AdkernelAdnAdapter::new(ep("adkerneladn")));
    reg!("admatic", crate::adapters6::AdmaticAdapter::new(ep("admatic")));
    reg!("adnuntius", crate::adapters6::AdnuntiusAdapter::new(ep("adnuntius")));
    reg!("adot", crate::adapters6::AdotAdapter::new(ep("adot")));
    reg!("adpone", crate::adapters6::AdponeAdapter::new(ep("adpone")));
    reg!("adprime", crate::adapters6::AdprimeAdapter::new(ep("adprime")));
    reg!("adquery", crate::adapters6::AdqueryAdapter::new(ep("adquery")));
    reg!("adrino", crate::adapters6::AdrinoAdapter::new(ep("adrino")));
    reg!("adtarget", crate::adapters6::AdtargetAdapter::new(ep("adtarget")));
    reg!("adtonos", crate::adapters6::AdtonosAdapter::new(ep("adtonos")));
    reg!("adtrgtme", crate::adapters6::AdtrgtmeAdapter::new(ep("adtrgtme")));
    reg!("aduptech", crate::adapters6::AduptechAdapter::new(ep("aduptech")));

    // adapters7/
    reg!("advangelists", crate::adapters7::AdvangelistsAdapter::new(ep("advangelists")));
    reg!("adverxo", crate::adapters7::AdverxoAdapter::new(ep("adverxo")));
    reg!("adview", crate::adapters7::AdviewAdapter::new(ep("adview")));
    reg!("adxcg", crate::adapters7::AdxcgAdapter::new(ep("adxcg")));
    reg!("adyoulike", crate::adapters7::AdyoulikeAdapter::new(ep("adyoulike")));
    reg!("afront", crate::adapters7::AfrontAdapter::new(ep("afront")));
    reg!("aidem", crate::adapters7::AidemAdapter::new(ep("aidem")));
    reg!("aja", crate::adapters7::AjaAdapter::new(ep("aja")));
    reg!("akcelo", crate::adapters7::AkceloAdapter::new(ep("akcelo")));
    reg!("algorix", crate::adapters7::AlgorixAdapter::new(ep("algorix")));
    reg!("alkimi", crate::adapters7::AlkimiAdapter::new(ep("alkimi")));
    reg!("alliance_gravity", crate::adapters7::AllianceGravityAdapter::new(ep("alliance_gravity")));
    reg!("amx", crate::adapters7::AmxAdapter::new(ep("amx")));
    reg!("apacdex", crate::adapters7::ApacdexAdapter::new(ep("apacdex")));
    reg!("appush", crate::adapters7::AppushAdapter::new(ep("appush")));
    reg!("aso", crate::adapters7::AsoAdapter::new(ep("aso")));
    reg!("audience_network", crate::adapters7::AudienceNetworkAdapter::new(ep("audienceNetwork")));
    reg!("automatad", crate::adapters7::AutomatadAdapter::new(ep("automatad")));
    reg!("avocet", crate::adapters7::AvocetAdapter::new(ep("avocet")));
    reg!("axis", crate::adapters7::AxisAdapter::new(ep("axis")));

    // adapters8/
    reg!("axonix", crate::adapters8::AxonixAdapter::new(ep("axonix")));
    reg!("beintoo", crate::adapters8::BeintooAdapter::new(ep("beintoo")));
    reg!("bematterfull", crate::adapters8::BematterfullAdapter::new(ep("bematterfull")));
    reg!("beop", crate::adapters8::BeopAdapter::new(ep("beop")));
    reg!("between", crate::adapters8::BetweenAdapter::new(ep("between")));
    reg!("beyondmedia", crate::adapters8::BeyondmediaAdapter::new(ep("beyondmedia")));
    reg!("bidmachine", crate::adapters8::BidmachineAdapter::new(ep("bidmachine")));
    reg!("bidmatic", crate::adapters8::BidmaticAdapter::new(ep("bidmatic")));
    reg!("bidmyadz", crate::adapters8::BidmyadzAdapter::new(ep("bidmyadz")));
    reg!("bidscube", crate::adapters8::BidscubeAdapter::new(ep("bidscube")));
    reg!("bidstack", crate::adapters8::BidstackAdapter::new(ep("bidstack")));
    reg!("bidtheatre", crate::adapters8::BidtheatreAdapter::new(ep("bidtheatre")));
    reg!("bigoad", crate::adapters8::BigoadAdapter::new(ep("bigoad")));
    reg!("blasto", crate::adapters8::BlastoAdapter::new(ep("blasto")));
    reg!("bliink", crate::adapters8::BliinkAdapter::new(ep("bliink")));
    reg!("blis", crate::adapters8::BlisAdapter::new(ep("blis")));
    reg!("blue", crate::adapters8::BlueAdapter::new(ep("blue")));
    reg!("bluesea", crate::adapters8::BluseaAdapter::new(ep("bluesea")));
    reg!("bmtm", crate::adapters8::BmtmAdapter::new(ep("bmtm")));
    reg!("compass", crate::adapters8::CompassAdapter::new(ep("compass")));

    // adapters9/
    reg!("boldwin", crate::adapters9::BoldwinAdapter::new(ep("boldwin")));
    reg!("boldwin_rapid", crate::adapters9::BoldwinRapidAdapter::new(ep("boldwin_rapid")));
    reg!("brave", crate::adapters9::BraveAdapter::new(ep("brave")));
    reg!("bwx", crate::adapters9::BwxAdapter::new(ep("bwx")));
    reg!("cadent_aperture_mx", crate::adapters9::CadentApertureMxAdapter::new(ep("cadent_aperture_mx")));
    reg!("ccx", crate::adapters9::CcxAdapter::new(ep("ccx")));
    reg!("clydo", crate::adapters9::ClydoAdapter::new(ep("clydo")));
    reg!("cointraffic", crate::adapters9::CointrafficAdapter::new(ep("cointraffic")));

    // adapters10/
    reg!("driftpixel", crate::adapters10::DriftpixelAdapter::new(ep("driftpixel")));
    reg!("e_volution", crate::adapters10::EvolutionAdapter::new(ep("e_volution")));
    reg!("elementaltv", crate::adapters10::ElementaltvAdapter::new(ep("elementaltv")));
    reg!("escalax", crate::adapters10::EscalaxAdapter::new(ep("escalax")));
    reg!("freewheelssp", crate::adapters10::FreewheelsspAdapter::new(ep("freewheelssp")));
    reg!("frvradn", crate::adapters10::FrvradnAdapter::new(ep("frvradn")));
    reg!("fwssp", crate::adapters10::FwsspAdapter::new(ep("fwssp")));
    reg!("globalsun", crate::adapters10::GlobalsunAdapter::new(ep("globalsun")));
    reg!("huaweiads", crate::adapters10::HuaweiadsAdapter::new(ep("huaweiads")));
    reg!("imds", crate::adapters10::ImdsAdapter::new(ep("imds")));

    // adapters11/
    reg!("logicad", crate::adapters11::LogicadAdapter::new(ep("logicad")));
    reg!("loyal", crate::adapters11::LoyalAdapter::new(ep("loyal")));
    reg!("mabidder", crate::adapters11::MabidderAdapter::new(ep("mabidder")));
    reg!("madsense", crate::adapters11::MadsenseAdapter::new(ep("madsense")));
    reg!("marsmedia", crate::adapters11::MarsmediaAdapter::new(ep("marsmedia")));
    reg!("mediafuse", crate::adapters11::MediafuseAdapter::new(ep("mediafuse")));
    reg!("mediago", crate::adapters11::MediagoAdapter::new(ep("mediago")));

    // adapters12/
    reg!("ogury", crate::adapters12::OguryAdapter::new(ep("ogury")));
    reg!("oms", crate::adapters12::OmsAdapter::new(ep("oms")));
    reg!("openweb", crate::adapters12::OpenwebAdapter::new(ep("openweb")));
    reg!("operaads", crate::adapters12::OperaadsAdapter::new(ep("operaads")));
    reg!("optidigital", crate::adapters12::OptidigitalAdapter::new(ep("optidigital")));
    reg!("oraki", crate::adapters12::OrakiAdapter::new(ep("oraki")));
    reg!("orbidder", crate::adapters12::OrbidderAdapter::new(ep("orbidder")));
    reg!("ownadx", crate::adapters12::OwnadxAdapter::new(ep("ownadx")));
    reg!("pgamssp", crate::adapters12::PgamsspAdapter::new(ep("pgamssp")));
    reg!("playdigo", crate::adapters12::PlaydigoAdapter::new(ep("playdigo")));

    // adapters13/
    reg!("roulax", crate::adapters13::RoulaxAdapter::new(ep("roulax")));
    reg!("sa_lunamedia", crate::adapters13::SaLunamediaAdapter::new(ep("sa_lunamedia")));
    reg!("seeding_alliance", crate::adapters13::SeedingAllianceAdapter::new(ep("seeding_alliance")));
    reg!("seedtag", crate::adapters13::SeedtagAdapter::new(ep("seedtag")));

    // adapters14/ — real implementations
    reg!("connatix", crate::adapters14::ConnatixAdapter::new(ep("connatix")));
    reg!("contxtful", crate::adapters14::ContxtfulAdapter::new(ep("contxtful")));
    reg!("flipp", crate::adapters14::FlippAdapter::new(ep("flipp")));
    reg!("invibes", crate::adapters14::InvibesAdapter::new(ep("invibes")));
    reg!("missena", crate::adapters14::MissenaAdapter::new(ep("missena")));
    reg!("msft", crate::adapters14::MsftAdapter::new(ep("msft")));
    reg!("nativery", crate::adapters14::NativeryAdapter::new(ep("nativery")));
    reg!("nextmillennium", crate::adapters14::NextmillenniumAdapter::new(ep("nextmillennium")));
    reg!("resetdigital", crate::adapters14::ResetdigitalAdapter::new(ep("resetdigital")));
    reg!("silverpush", crate::adapters14::SilverpushAdapter::new(ep("silverpush")));
    reg!("telaria", crate::adapters14::TelariaAdapter::new(ep("telaria")));
    reg!("unicorn", crate::adapters14::UnicornAdapter::new(ep("unicorn")));
    reg!("yieldlab", crate::adapters14::YieldlabAdapter::new(ep("yieldlab")));
    // adapters14/ — remaining generic pass-throughs
    gen!("across33");      // duplicate handled above as "33across"
    gen!("coinzilla");
    gen!("concert");
    gen!("copper6ssp");
    gen!("cpmstar");
    gen!("cwire");
    gen!("decenterads");
    gen!("definemedia");
    gen!("dianomi");
    gen!("displayio");
    gen!("edge226");
    gen!("exco");
    gen!("feedad");
    gen!("flatads");
    gen!("impactify");
    gen!("intenze");
    gen!("interactiveoffers");
    gen!("iqx");
    gen!("iqzone");
    gen!("kiviads");
    gen!("krushmedia");
    gen!("kueezrtb");
    gen!("lemmadigital");
    gen!("limelight_digital");
    gen!("lm_kiviads");
    gen!("logan");
    gen!("mediasquare");
    gen!("melozen");
    gen!("metax");
    gen!("mgid_x");
    gen!("mobfoxpb");
    gen!("mobilefuse");
    gen!("mobkoi");
    gen!("motorik");
    gen!("nexx360");
    gen!("pubnative");
    gen!("pubrise");
    gen!("pwbid");
    gen!("qt");
    gen!("readpeak");
    gen!("rediads");
    gen!("relevantdigital");
    gen!("rise");
    gen!("showheroes");
    gen!("silvermob");
    gen!("smarthub");
    gen!("smartrtb");
    gen!("smartx");
    gen!("smartyads");
    gen!("smilewanted");
    gen!("smoot");
    gen!("smrtconnect");
    gen!("sovrn_xsp");
    gen!("sparteo");
    gen!("sspbc");
    gen!("startio");
    gen!("stroeer_core");
    gen!("tappx");
    gen!("teqblaze");
    gen!("theadx");
    gen!("thetradedesk");
    gen!("tpmn");
    gen!("tradplus");
    gen!("trafficgate");
    gen!("trustedstack");
    gen!("trustx");
    gen!("ucfunnel");
    gen!("undertone");
    gen!("videobyte");
    gen!("videoheroes");
    gen!("vidoomy");
    gen!("visiblemeasures");
    gen!("visx");
    gen!("vox");
    gen!("vrtcal");
    gen!("vungle");
    gen!("xeworks");
    gen!("yahoo_ads");
    gen!("yeahmobi");
    gen!("yieldmo");
    gen!("yieldone");
    gen!("zentotem");
    gen!("zeroclickfraud");
    gen!("zeta_global_ssp");
    gen!("zmaticoo");

    // Additional missing bidders (aliases and generics)
    gen!("152media");
    gen!("1accord");
    gen!("360playvid");
    gen!("adastra");
    gen!("addigi");
    gen!("adkernelAdn");
    gen!("admaticde");
    gen!("adport");
    gen!("ads_interactive");
    gen!("adsinteractive");
    gen!("adsyield");
    gen!("adt");
    gen!("adtg_org");
    gen!("alchemyx");
    gen!("altstar");
    gen!("anzuExchange");
    gen!("appStockSSP");
    gen!("appstock");
    gen!("artechnology");
    gen!("bcmint");
    gen!("bidfuse");
    gen!("bidgency");
    gen!("bidsmind");
    gen!("connektai");
    gen!("copper6");
    gen!("easybid");
    gen!("embimedia");
    gen!("epsilon");
    gen!("equativ");
    gen!("evtech");
    gen!("felixads");
    gen!("filmzie");
    gen!("finative");
    gen!("freewheel-ssp");
    gen!("gravite");
    gen!("greedygame");
    gen!("iionads");
    gen!("indicue");
    gen!("jambojar");
    gen!("janet");
    gen!("jdpmedia");
    gen!("kuantyx");
    gen!("limelightDigital");
    gen!("magnite");
    gen!("markapp");
    gen!("mediayo");
    gen!("mgidX");
    gen!("monetixads");
    gen!("netaddiction");
    gen!("nuba");
    gen!("omnidex");
    gen!("orangeclickmedia");
    gen!("oveeo");
    gen!("performist");
    gen!("pgam");
    gen!("pinkLion");
    gen!("pixad");
    gen!("prismassp");
    gen!("programmaticX");
    gen!("progx");
    gen!("quantumdex");
    gen!("radiantfusion");
    gen!("robustApps");
    gen!("rocketlab");
    gen!("rtbdemand");
    gen!("rxnetwork");
    gen!("screencore");
    gen!("seedingAlliance");
    gen!("showheroes-bs");
    gen!("showheroesBs");
    gen!("smootai");
    gen!("sovrnXsp");
    gen!("sspBC");
    gen!("streamkey");
    gen!("streamlyn");
    gen!("streamvision");
    gen!("stroeerCore");
    gen!("suntContent");
    gen!("tagoras");
    gen!("tgm");
    gen!("tredio");
    gen!("ttd");
    gen!("valueimpression");
    gen!("velonium");
    gen!("viewdeos");
    gen!("xapads");
    gen!("xtrmqb");
    gen!("yahooAds");
    gen!("yahooAdvertising");
    gen!("yahoossp");
    gen!("yobee");
    gen!("adform");
    gen!("adinify");
    gen!("adipolo");

    m
}
