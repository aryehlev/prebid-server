use std::collections::HashMap;
use crate::Bidder;

/// Directories to search for bidder-info YAML files, in order of preference.
const BIDDER_INFO_DIRS: &[&str] = &[
    "/home/user/prebid-server/static/bidder-info",
    "static/bidder-info",
    "../static/bidder-info",
];

/// Find the bidder-info directory that actually exists on disk.
fn find_bidder_info_dir() -> Option<&'static str> {
    for dir in BIDDER_INFO_DIRS {
        if std::path::Path::new(dir).is_dir() {
            return Some(dir);
        }
    }
    None
}

fn read_endpoint(name: &str, default: &str) -> String {
    for dir in BIDDER_INFO_DIRS {
        let path = format!("{}/{}.yaml", dir, name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("endpoint:") && !line.starts_with('#') {
                    let val = line.trim_start_matches("endpoint:").trim()
                        .trim_matches('"').trim_matches('\'');
                    if !val.is_empty() {
                        return val.to_string();
                    }
                }
            }
        }
    }
    default.to_string()
}

/// Read the `aliasOf` field from a bidder's YAML file, if present.
#[cfg(test)]
fn read_alias_of(name: &str) -> Option<String> {
    for dir in BIDDER_INFO_DIRS {
        let path = format!("{}/{}.yaml", dir, name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("aliasOf:") {
                    let val = line.trim_start_matches("aliasOf:").trim()
                        .trim_matches('"').trim_matches('\'');
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Scan the bidder-info directory for all YAML-defined aliases.
/// Returns a list of (alias_name, parent_bidder_name) pairs.
fn discover_yaml_aliases() -> Vec<(String, String)> {
    let dir = match find_bidder_info_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut aliases = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let bidder_name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with("aliasOf:") {
                    let parent = line.trim_start_matches("aliasOf:").trim()
                        .trim_matches('"').trim_matches('\'');
                    if !parent.is_empty() {
                        aliases.push((bidder_name.clone(), parent.to_string()));
                    }
                    break;
                }
            }
        }
    }
    aliases
}

fn ep(name: &str) -> String { read_endpoint(name, "") }

/// A builder function that creates a bidder adapter given an endpoint URL.
/// This mirrors Go's `adapters.Builder` type, enabling aliases to reuse
/// their parent bidder's adapter logic with a different endpoint.
type AdapterBuilder = Box<dyn Fn(String) -> Box<dyn Bidder>>;

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

    // Builder registry: maps canonical bidder names to factory functions.
    // When a YAML alias references a parent, we look up the parent's builder
    // here and create a new adapter instance with the alias's endpoint.
    // This mirrors Go's `setAliasBuilder` in exchange/adapter_util.go.
    let mut builders: HashMap<String, AdapterBuilder> = HashMap::new();

    // Register a bidder with both an adapter instance and a builder function.
    macro_rules! reg {
        ($key:expr, $adapter:expr) => { m.insert($key.to_string(), Box::new($adapter)); };
    }
    macro_rules! reg_with_builder {
        ($key:expr, $adapter:expr, $builder:expr) => {
            m.insert($key.to_string(), Box::new($adapter));
            builders.insert($key.to_string(), Box::new($builder));
        };
    }
    macro_rules! gen {
        ($key:expr) => { m.insert($key.to_string(), GenericAdapter::boxed(ep($key))); };
    }

    // adapters/ — real implementations
    // Adapters that are known parents of YAML aliases use reg_with_builder!
    // so aliases can reuse the parent's adapter logic with a different endpoint.
    reg_with_builder!("appnexus", crate::adapters::AppnexusAdapter::new(ep("appnexus")),
        |endpoint| Box::new(crate::adapters::AppnexusAdapter::new(endpoint)));
    reg_with_builder!("sovrn", crate::adapters::SovrnAdapter::new(ep("sovrn")),
        |endpoint| Box::new(crate::adapters::SovrnAdapter::new(endpoint)));
    reg_with_builder!("rubicon", crate::adapters::RubiconAdapter::new(ep("rubicon"), String::new(), String::new()),
        |endpoint| Box::new(crate::adapters::RubiconAdapter::new(endpoint, String::new(), String::new())));
    reg_with_builder!("ix", crate::adapters::IxAdapter::new(ep("ix")),
        |endpoint| Box::new(crate::adapters::IxAdapter::new(endpoint)));
    reg_with_builder!("openx", crate::adapters::OpenxAdapter::new(ep("openx"), "openx".to_string()),
        |endpoint| Box::new(crate::adapters::OpenxAdapter::new(endpoint, "openx".to_string())));
    reg_with_builder!("pubmatic", crate::adapters::PubmaticAdapter::new(ep("pubmatic")),
        |endpoint| Box::new(crate::adapters::PubmaticAdapter::new(endpoint)));
    reg_with_builder!("33across", crate::adapters::Across33Adapter::new(ep("33across")),
        |endpoint| Box::new(crate::adapters::Across33Adapter::new(endpoint)));
    reg_with_builder!("beachfront", crate::adapters::BeachfrontAdapter::new(
        ep("beachfront"),
        "https://reachms.bfmio.com/bid.json?exchange_id".to_string(),
    ), |endpoint| Box::new(crate::adapters::BeachfrontAdapter::new(endpoint, "https://reachms.bfmio.com/bid.json?exchange_id".to_string())));
    reg_with_builder!("criteo", crate::adapters::CriteoAdapter::new(ep("criteo")),
        |endpoint| Box::new(crate::adapters::CriteoAdapter::new(endpoint)));
    reg_with_builder!("sharethrough", crate::adapters::SharethroughAdapter::new(ep("sharethrough")),
        |endpoint| Box::new(crate::adapters::SharethroughAdapter::new(endpoint)));

    // adapters2/
    reg_with_builder!("smartadserver", crate::adapters2::SmartadserverAdapter::new(ep("smartadserver")),
        |endpoint| Box::new(crate::adapters2::SmartadserverAdapter::new(endpoint)));
    reg!("triplelift", crate::adapters2::TripleliftAdapter::new(ep("triplelift")));
    reg!("taboola", crate::adapters2::TaboolaAdapter::new(ep("taboola")));
    reg!("outbrain", crate::adapters2::OutbrainAdapter::new(ep("outbrain")));
    reg!("grid", crate::adapters2::GridAdapter::new(ep("grid")));
    reg!("smaato", crate::adapters2::SmaatoAdapter::new(ep("smaato")));
    reg!("richaudience", crate::adapters2::RichaudienceAdapter::new(ep("richaudience")));
    reg!("pulsepoint", crate::adapters2::PulsepointAdapter::new(ep("pulsepoint")));
    reg!("rtbhouse", crate::adapters2::RtbhouseAdapter::new(ep("rtbhouse")));
    reg_with_builder!("conversant", crate::adapters2::ConversantAdapter::new(ep("conversant")),
        |endpoint| Box::new(crate::adapters2::ConversantAdapter::new(endpoint)));
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
    reg_with_builder!("adtelligent", crate::adapters3::AdtelligentAdapter::new(ep("adtelligent")),
        |endpoint| Box::new(crate::adapters3::AdtelligentAdapter::new(endpoint)));
    reg_with_builder!("adkernel", crate::adapters3::AdkernelAdapter::new(ep("adkernel")),
        |endpoint| Box::new(crate::adapters3::AdkernelAdapter::new(endpoint)));
    reg!("admixer", crate::adapters3::AdmixerAdapter::new(ep("admixer")));
    reg!("adman", crate::adapters3::AdmanAdapter::new(ep("adman")));
    reg!("nativo", crate::adapters3::NativoAdapter::new(ep("nativo")));
    reg!("nobid", crate::adapters3::NobidAdapter::new(ep("nobid")));
    reg!("eplanning", crate::adapters3::EplanningAdapter::new(ep("eplanning")));
    reg!("loopme", crate::adapters3::LoopmeAdapter::new(ep("loopme")));
    reg_with_builder!("vidazoo", crate::adapters3::VidazooAdapter::new(ep("vidazoo")),
        |endpoint| Box::new(crate::adapters3::VidazooAdapter::new(endpoint)));
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
    reg_with_builder!("adf", crate::adapters6::AdfAdapter::new(ep("adf")),
        |endpoint| Box::new(crate::adapters6::AdfAdapter::new(endpoint)));
    reg!("adgeneration", crate::adapters6::AdgenerationAdapter::new(ep("adgeneration")));
    reg!("adhese", crate::adapters6::AdheseAdapter::new(ep("adhese")));
    reg!("adkernelAdn", crate::adapters6::AdkernelAdnAdapter::new(ep("adkernelAdn")));
    reg_with_builder!("admatic", crate::adapters6::AdmaticAdapter::new(ep("admatic")),
        |endpoint| Box::new(crate::adapters6::AdmaticAdapter::new(endpoint)));
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
    reg_with_builder!("adverxo", crate::adapters7::AdverxoAdapter::new(ep("adverxo")),
        |endpoint| Box::new(crate::adapters7::AdverxoAdapter::new(endpoint)));
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
    reg_with_builder!("apacdex", crate::adapters7::ApacdexAdapter::new(ep("apacdex")),
        |endpoint| Box::new(crate::adapters7::ApacdexAdapter::new(endpoint)));
    reg!("appush", crate::adapters7::AppushAdapter::new(ep("appush")));
    reg_with_builder!("aso", crate::adapters7::AsoAdapter::new(ep("aso")),
        |endpoint| Box::new(crate::adapters7::AsoAdapter::new(endpoint)));
    reg!("audienceNetwork", crate::adapters7::AudienceNetworkAdapter::new(ep("audienceNetwork")));
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
    reg!("bluesea", crate::adapters8::BlueseaAdapter::new(ep("bluesea")));
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
    reg_with_builder!("freewheelssp", crate::adapters10::FreewheelsspAdapter::new(ep("freewheelssp")),
        |endpoint| Box::new(crate::adapters10::FreewheelsspAdapter::new(endpoint)));
    reg!("frvradn", crate::adapters10::FrvradnAdapter::new(ep("frvradn")));
    reg!("fwssp", crate::adapters10::FwsspAdapter::new(ep("fwssp")));
    reg!("globalsun", crate::adapters10::GlobalsunAdapter::new(ep("globalsun")));
    reg!("huaweiads", crate::adapters10::HuaweiadsAdapter::new(ep("huaweiads")));
    reg_with_builder!("imds", crate::adapters10::ImdsAdapter::new(ep("imds")),
        |endpoint| Box::new(crate::adapters10::ImdsAdapter::new(endpoint)));

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
    reg!("across33", crate::adapters14::Across33Adapter::new(ep("across33")));      // duplicate handled above as "33across"
    reg!("coinzilla", crate::adapters14::CoinzillaAdapter::new(ep("coinzilla")));
    reg!("concert", crate::adapters14::ConcertAdapter::new(ep("concert")));
    reg!("copper6ssp", crate::adapters14::Copper6sspAdapter::new(ep("copper6ssp")));
    reg!("cpmstar", crate::adapters14::CpmstarAdapter::new(ep("cpmstar")));
    reg!("cwire", crate::adapters14::CwireAdapter::new(ep("cwire")));
    reg!("decenterads", crate::adapters14::DecenteradsAdapter::new(ep("decenterads")));
    reg!("definemedia", crate::adapters14::DefinemediaAdapter::new(ep("definemedia")));
    reg!("dianomi", crate::adapters14::DianomiAdapter::new(ep("dianomi")));
    reg!("displayio", crate::adapters14::DisplayioAdapter::new(ep("displayio")));
    reg!("edge226", crate::adapters14::Edge226Adapter::new(ep("edge226")));
    reg!("exco", crate::adapters14::ExcoAdapter::new(ep("exco")));
    reg!("feedad", crate::adapters14::FeedadAdapter::new(ep("feedad")));
    reg!("flatads", crate::adapters14::FlatadsAdapter::new(ep("flatads")));
    reg!("impactify", crate::adapters14::ImpactifyAdapter::new(ep("impactify")));
    reg!("intenze", crate::adapters14::IntenzeAdapter::new(ep("intenze")));
    reg!("interactiveoffers", crate::adapters14::InteractiveoffersAdapter::new(ep("interactiveoffers")));
    reg!("iqx", crate::adapters14::IqxAdapter::new(ep("iqx")));
    reg!("iqzone", crate::adapters14::IqzoneAdapter::new(ep("iqzone")));
    reg!("kiviads", crate::adapters14::KiviadsAdapter::new(ep("kiviads")));
    reg!("krushmedia", crate::adapters14::KrushmediaAdapter::new(ep("krushmedia")));
    reg!("kueezrtb", crate::adapters14::KueezrtbAdapter::new(ep("kueezrtb")));
    reg!("lemmadigital", crate::adapters14::LemmadigitalAdapter::new(ep("lemmadigital")));
    reg!("limelight_digital", crate::adapters14::LimelightDigitalAdapter::new(ep("limelight_digital")));
    reg!("lm_kiviads", crate::adapters14::LmKiviadsAdapter::new(ep("lm_kiviads")));
    reg!("logan", crate::adapters14::LoganAdapter::new(ep("logan")));
    reg!("mediasquare", crate::adapters14::MediasquareAdapter::new(ep("mediasquare")));
    reg!("melozen", crate::adapters14::MelozenAdapter::new(ep("melozen")));
    reg!("metax", crate::adapters14::MetaxAdapter::new(ep("metax")));
    reg!("mgid_x", crate::adapters14::MgidXAdapter::new(ep("mgid_x")));
    reg!("mobfoxpb", crate::adapters14::MobfoxpbAdapter::new(ep("mobfoxpb")));
    reg!("mobilefuse", crate::adapters14::MobilefuseAdapter::new(ep("mobilefuse")));
    reg!("mobkoi", crate::adapters14::MobkoiAdapter::new(ep("mobkoi")));
    reg!("motorik", crate::adapters14::MotorikAdapter::new(ep("motorik")));
    reg_with_builder!("nexx360", crate::adapters14::Nexx360Adapter::new(ep("nexx360")),
        |endpoint| Box::new(crate::adapters14::Nexx360Adapter::new(endpoint)));
    reg!("pubnative", crate::adapters14::PubnativeAdapter::new(ep("pubnative")));
    reg!("pubrise", crate::adapters14::PubriseAdapter::new(ep("pubrise")));
    reg!("pwbid", crate::adapters14::PwbidAdapter::new(ep("pwbid")));
    reg!("qt", crate::adapters14::QtAdapter::new(ep("qt")));
    reg!("readpeak", crate::adapters14::ReadpeakAdapter::new(ep("readpeak")));
    reg!("rediads", crate::adapters14::RediadsAdapter::new(ep("rediads")));
    reg!("relevantdigital", crate::adapters14::RelevantdigitalAdapter::new(ep("relevantdigital")));
    reg!("rise", crate::adapters14::RiseAdapter::new(ep("rise")));
    reg_with_builder!("showheroes", crate::adapters14::ShowheroesAdapter::new(ep("showheroes")),
        |endpoint| Box::new(crate::adapters14::ShowheroesAdapter::new(endpoint)));
    reg!("silvermob", crate::adapters14::SilvermobAdapter::new(ep("silvermob")));
    reg_with_builder!("smarthub", crate::adapters14::SmarthubAdapter::new(ep("smarthub")),
        |endpoint| Box::new(crate::adapters14::SmarthubAdapter::new(endpoint)));
    reg!("smartrtb", crate::adapters14::SmartrtbAdapter::new(ep("smartrtb")));
    reg!("smartx", crate::adapters14::SmartxAdapter::new(ep("smartx")));
    reg!("smartyads", crate::adapters14::SmartyadsAdapter::new(ep("smartyads")));
    reg!("smilewanted", crate::adapters14::SmilewantedAdapter::new(ep("smilewanted")));
    reg!("smoot", crate::adapters14::SmootAdapter::new(ep("smoot")));
    reg!("smrtconnect", crate::adapters14::SmrtconnectAdapter::new(ep("smrtconnect")));
    reg!("sovrn_xsp", crate::adapters14::SovrnXspAdapter::new(ep("sovrn_xsp")));
    reg!("sparteo", crate::adapters14::SparteoAdapter::new(ep("sparteo")));
    reg!("sspbc", crate::adapters14::SspBcAdapter::new(ep("sspbc")));
    reg!("startio", crate::adapters14::StartioAdapter::new(ep("startio")));
    reg!("stroeer_core", crate::adapters14::StroeerCoreAdapter::new(ep("stroeer_core")));
    reg!("tappx", crate::adapters14::TappxAdapter::new(ep("tappx")));
    reg_with_builder!("teqblaze", crate::adapters14::TeqblazeAdapter::new(ep("teqblaze")),
        |endpoint| Box::new(crate::adapters14::TeqblazeAdapter::new(endpoint)));
    reg!("theadx", crate::adapters14::TheadxAdapter::new(ep("theadx")));
    reg_with_builder!("thetradedesk", crate::adapters14::ThetradedeskAdapter::new(ep("thetradedesk")),
        |endpoint| Box::new(crate::adapters14::ThetradedeskAdapter::new(endpoint)));
    reg!("tpmn", crate::adapters14::TpmnAdapter::new(ep("tpmn")));
    reg!("tradplus", crate::adapters14::TradplusAdapter::new(ep("tradplus")));
    reg!("trafficgate", crate::adapters14::TrafficgateAdapter::new(ep("trafficgate")));
    reg!("trustedstack", crate::adapters14::TrustedstackAdapter::new(ep("trustedstack")));
    reg!("trustx", crate::adapters14::TrustxAdapter::new(ep("trustx")));
    reg!("ucfunnel", crate::adapters14::UcfunnelAdapter::new(ep("ucfunnel")));
    reg!("undertone", crate::adapters14::UndertoneAdapter::new(ep("undertone")));
    reg!("videobyte", crate::adapters14::VideobyteAdapter::new(ep("videobyte")));
    reg!("videoheroes", crate::adapters14::VideoHeroesAdapter::new(ep("videoheroes")));
    reg!("vidoomy", crate::adapters14::VidoomyAdapter::new(ep("vidoomy")));
    reg!("visiblemeasures", crate::adapters14::VisiblemeasuresAdapter::new(ep("visiblemeasures")));
    reg!("visx", crate::adapters14::VisxAdapter::new(ep("visx")));
    reg!("vox", crate::adapters14::VoxAdapter::new(ep("vox")));
    reg!("vrtcal", crate::adapters14::VrtcalAdapter::new(ep("vrtcal")));
    reg!("vungle", crate::adapters14::VungleAdapter::new(ep("vungle")));
    reg_with_builder!("xeworks", crate::adapters14::XeworksAdapter::new(ep("xeworks")),
        |endpoint| Box::new(crate::adapters14::XeworksAdapter::new(endpoint)));
    reg!("yahoo_ads", crate::adapters14::YahooAdsAdapter::new(ep("yahoo_ads")));
    reg!("yeahmobi", crate::adapters14::YeahmobiAdapter::new(ep("yeahmobi")));
    reg!("yieldmo", crate::adapters14::YieldmoAdapter::new(ep("yieldmo")));
    reg!("yieldone", crate::adapters14::YieldoneAdapter::new(ep("yieldone")));
    reg!("zentotem", crate::adapters14::ZentotemAdapter::new(ep("zentotem")));
    reg!("zeroclickfraud", crate::adapters14::ZeroclickfraudAdapter::new(ep("zeroclickfraud")));
    reg!("zeta_global_ssp", crate::adapters14::ZetaGlobalSspAdapter::new(ep("zeta_global_ssp")));
    reg!("zmaticoo", crate::adapters14::ZmaticooAdapter::new(ep("zmaticoo")));

    // Non-alias bidders without custom adapters (use generic fallback)
    gen!("sspBC");

    // Explicitly registered adapters with variant/alternate names
    reg_with_builder!("limelightDigital", crate::adapters14::LimelightDigitalAdapter::new(ep("limelightDigital")),
        |endpoint| Box::new(crate::adapters14::LimelightDigitalAdapter::new(endpoint)));
    reg!("mgidX", crate::adapters14::MgidXAdapter::new(ep("mgid_x")));
    reg_with_builder!("seedingAlliance", crate::adapters14::SeedingAllianceAdapter::new(ep("seedingAlliance")),
        |endpoint| Box::new(crate::adapters14::SeedingAllianceAdapter::new(endpoint)));
    reg!("sovrnXsp", crate::adapters14::SovrnXspAdapter::new(ep("sovrnXsp")));
    reg_with_builder!("yahooAds", crate::adapters14::YahooAdsAdapter::new(ep("yahooAds")),
        |endpoint| Box::new(crate::adapters14::YahooAdsAdapter::new(endpoint)));
    reg!("freewheel-ssp", crate::adapters10::FreewheelsspAdapter::new(ep("freewheelssp")));

    // -----------------------------------------------------------------------
    // Alias resolution: scan YAML bidder-info files for `aliasOf` entries
    // and create adapter instances using the parent bidder's builder.
    // This mirrors Go's `setAliasBuilder` in exchange/adapter_util.go.
    //
    // Any bidder with `aliasOf: <parent>` in its YAML file will get an
    // instance of the parent's adapter (with the alias's own endpoint).
    // Bidders without a YAML alias or without a registered parent builder
    // fall through to the generic adapter.
    // -----------------------------------------------------------------------
    let yaml_aliases = discover_yaml_aliases();
    for (alias_name, parent_name) in &yaml_aliases {
        // Skip if this alias is already explicitly registered above.
        if m.contains_key(alias_name) {
            continue;
        }
        let alias_endpoint = ep(alias_name);
        if let Some(builder) = builders.get(parent_name) {
            // Use the parent's builder to create an adapter with the alias's endpoint.
            // If the alias has no endpoint in its YAML, use the parent's endpoint.
            let endpoint = if alias_endpoint.is_empty() {
                ep(parent_name)
            } else {
                alias_endpoint
            };
            m.insert(alias_name.clone(), builder(endpoint));
        } else {
            // Parent builder not registered — fall back to generic adapter.
            let endpoint = if alias_endpoint.is_empty() {
                ep(parent_name)
            } else {
                alias_endpoint
            };
            m.insert(alias_name.clone(), GenericAdapter::boxed(endpoint));
        }
    }

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_alias_of_returns_none_for_non_alias() {
        // appnexus is not an alias
        assert!(read_alias_of("appnexus").is_none());
    }

    #[test]
    fn test_read_alias_of_returns_parent_for_alias() {
        // magnite is aliasOf rubicon
        if let Some(parent) = read_alias_of("magnite") {
            assert_eq!(parent, "rubicon");
        }
        // equativ is aliasOf smartadserver
        if let Some(parent) = read_alias_of("equativ") {
            assert_eq!(parent, "smartadserver");
        }
    }

    #[test]
    fn test_discover_yaml_aliases_finds_aliases() {
        let aliases = discover_yaml_aliases();
        if find_bidder_info_dir().is_some() {
            assert!(!aliases.is_empty(), "should discover YAML aliases");
            let has_magnite = aliases.iter().any(|(name, parent)| name == "magnite" && parent == "rubicon");
            assert!(has_magnite, "magnite -> rubicon alias should be discovered");
        }
    }

    #[test]
    fn test_build_adapter_map_aliases_use_parent_adapter() {
        let map = build_adapter_map();

        // magnite is an alias of rubicon — it should be registered
        assert!(map.contains_key("magnite"), "magnite alias should be in adapter map");

        // equativ is an alias of smartadserver
        assert!(map.contains_key("equativ"), "equativ alias should be in adapter map");

        // The parent bidders should also exist
        assert!(map.contains_key("rubicon"), "rubicon parent should be in adapter map");
        assert!(map.contains_key("smartadserver"), "smartadserver parent should be in adapter map");
    }

    #[test]
    fn test_build_adapter_map_alias_produces_valid_requests() {
        let map = build_adapter_map();

        // magnite (alias of rubicon) should produce requests using RubiconAdapter logic
        if let Some(adapter) = map.get("magnite") {
            let mut imp = openrtb::Imp::default();
            imp.id = "imp1".to_string();
            imp.banner = Some(openrtb::Banner::default());
            imp.ext = Some(serde_json::json!({"bidder": {"accountId": 1, "siteId": 2, "zoneId": 3}}));
            let req = openrtb::BidRequest {
                id: "test-alias".to_string(),
                imp: vec![imp],
                ..Default::default()
            };
            let (requests, _errors) = adapter.make_requests(&req, &crate::ExtraRequestInfo::default());
            // RubiconAdapter adds an Authorization header; GenericAdapter does not.
            if !requests.is_empty() {
                let has_auth = requests[0].headers.contains_key("Authorization");
                assert!(has_auth, "magnite (alias of rubicon) should use RubiconAdapter logic with auth header");
            }
        }
    }
}
