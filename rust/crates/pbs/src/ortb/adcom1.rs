//! AdCOM 1.0 enumerations (Go `github.com/risecodes/openrtb/adcom1`).
//!
//! Only the code lists are ported. The AdCOM object model (`Ad`, `Placement`, ...) belongs to
//! OpenRTB 3 and the seller never uses it. Constant names drop the Go type prefix:
//! `adcom1.AttrAudioAuto` is `CreativeAttribute::AUDIO_AUTO`.

use super::de::ortb_code;

ortb_code! {
    /// List: Agent Types (`eids.uids.atype`).
    AgentType(i64) {
        WEB = 1,
        APP = 2,
        PERSON = 3,
    }
}

ortb_code! {
    /// List: API Frameworks.
    ApiFramework(i64) {
        VPAID_10 = 1,
        VPAID_20 = 2,
        MRAID_10 = 3,
        ORMMA = 4,
        MRAID_20 = 5,
        MRAID_30 = 6,
        OMID_10 = 7,
        SIMID_10 = 8,
        SIMID_11 = 9,
    }
}

ortb_code! {
    /// List: Auto Refresh Triggers.
    AutoRefreshTrigger(i8) {
        UNKNOWN = 0,
        USER_ACTION = 1,
        EVENT = 2,
        TIME = 3,
    }
}

ortb_code! {
    /// List: Category Taxonomies.
    CategoryTaxonomy(i64) {
        IAB_CONTENT_10 = 1,
        IAB_CONTENT_20 = 2,
        IAB_PRODUCT_10 = 3,
        IAB_AUDIENCE_11 = 4,
        IAB_CONTENT_21 = 5,
        IAB_CONTENT_22 = 6,
        IAB_CONTENT_30 = 7,
    }
}

ortb_code! {
    /// List: Companion Types.
    CompanionType(i8) {
        STATIC = 1,
        HTML = 2,
        IFRAME = 3,
    }
}

ortb_code! {
    /// List: Connection Types.
    ConnectionType(i8) {
        UNKNOWN = 0,
        ETHERNET = 1,
        WIFI = 2,
        CELLULAR = 3,
        CELLULAR_2G = 4,
        CELLULAR_3G = 5,
        CELLULAR_4G = 6,
        CELLULAR_5G = 7,
    }
}

ortb_code! {
    /// List: Content Contexts.
    ContentContext(i8) {
        VIDEO = 1,
        GAME = 2,
        MUSIC = 3,
        APP = 4,
        TEXT = 5,
        OTHER = 6,
        UNKNOWN = 7,
    }
}

ortb_code! {
    /// List: Creative Attributes.
    CreativeAttribute(i64) {
        AUDIO_AUTO = 1,
        AUDIO_USER = 2,
        EXPANDABLE_AUTO = 3,
        EXPANDABLE_USER_CLICK = 4,
        EXPANDABLE_USER_ROLLOVER = 5,
        VIDEO_AUTO = 6,
        VIDEO_USER = 7,
        POP = 8,
        PROVOCATIVE = 9,
        EXTREME_ANIMATION = 10,
        SURVEY = 11,
        TEXT_ONLY = 12,
        INTERACTIVE = 13,
        WINDOWS_DIALOG = 14,
        HAS_AUDIO_TOGGLE_BUTTON = 15,
        HAS_SKIP_BUTTON = 16,
        FLASH = 17,
        RESPONSIVE = 18,
    }
}

ortb_code! {
    /// List: Delivery Methods.
    DeliveryMethod(i8) {
        STREAMING = 1,
        PROGRESSIVE = 2,
        DOWNLOAD = 3,
    }
}

ortb_code! {
    /// List: Device Types.
    DeviceType(i8) {
        MOBILE = 1,
        PC = 2,
        TV = 3,
        PHONE = 4,
        TABLET = 5,
        CONNECTED = 6,
        SET_TOP_BOX = 7,
        OOH = 8,
    }
}

ortb_code! {
    /// List: DOOH Multiplier Measurement Source Types.
    DoohMultiplierMeasurementSourceType(i8) {
        UNKNOWN = 0,
        MEASUREMENT_VENDOR_PROVIDED = 1,
        PUBLISHER_PROVIDED = 2,
        EXCHANGE_PROVIDED = 3,
    }
}

ortb_code! {
    /// List: DOOH Venue Taxonomies. Go's `Val()` on a nil pointer returns `OPEN_OOH_10`.
    DoohVenueTaxonomy(i64) {
        ADCOM = 0,
        OPEN_OOH_10 = 1,
        DPAA = 2,
        DMI_11 = 3,
        OMA_JAN_2022 = 4,
        OPEN_OOH_11 = 5,
    }
}

ortb_code! {
    /// List: Expandable Directions.
    ExpandableDirection(i8) {
        LEFT = 1,
        RIGHT = 2,
        UP = 3,
        DOWN = 4,
        FULL_SCREEN = 5,
        RESIZE = 6,
    }
}

ortb_code! {
    /// List: Feed Types.
    FeedType(i8) {
        MUSIC_SERVICE = 1,
        RADIO_BROADCAST = 2,
        PODCAST = 3,
        CATCH_UP_RADIO = 4,
        WEB_RADIO = 5,
        VIDEO_GAME = 6,
        TEXT_TO_SPEECH = 7,
    }
}

ortb_code! {
    /// List: IP Location Services.
    IpLocationService(i8) {
        IP2LOCATION = 1,
        NEUSTAR = 2,
        MAXMIND = 3,
        NETACUITY = 4,
    }
}

ortb_code! {
    /// List: Linearity Modes.
    LinearityMode(i8) {
        LINEAR = 1,
        NON_LINEAR = 2,
    }
}

ortb_code! {
    /// List: Location Types.
    LocationType(i8) {
        GPS = 1,
        IP = 2,
        USER_PROVIDED = 3,
    }
}

ortb_code! {
    /// List: Creative Subtypes - Audio/Video (OpenRTB 2 `protocols`).
    MediaCreativeSubtype(i8) {
        VAST_10 = 1,
        VAST_20 = 2,
        VAST_30 = 3,
        VAST_10_WRAPPER = 4,
        VAST_20_WRAPPER = 5,
        VAST_30_WRAPPER = 6,
        VAST_40 = 7,
        VAST_40_WRAPPER = 8,
        DAAST_10 = 9,
        DAAST_10_WRAPPER = 10,
        VAST_41 = 11,
        VAST_41_WRAPPER = 12,
        VAST_42 = 13,
        VAST_42_WRAPPER = 14,
    }
}

ortb_code! {
    /// List: Media Ratings.
    MediaRating(i8) {
        ALL = 1,
        OVER_12 = 2,
        MATURE = 3,
    }
}

ortb_code! {
    /// List: Placement Positions.
    PlacementPosition(i8) {
        UNKNOWN = 0,
        ABOVE_FOLD = 1,
        LOCKED = 2,
        BELOW_FOLD = 3,
        HEADER = 4,
        FOOTER = 5,
        SIDE_BAR = 6,
        FULL_SCREEN = 7,
    }
}

ortb_code! {
    /// List: Playback Cessation Modes.
    PlaybackCessationMode(i8) {
        COMPLETION = 1,
        LEAVING_VIEWPORT = 2,
        FLOATING = 3,
    }
}

ortb_code! {
    /// List: Playback Methods.
    PlaybackMethod(i8) {
        PAGE_LOAD_SOUND_ON = 1,
        PAGE_LOAD_SOUND_OFF = 2,
        CLICK_SOUND_ON = 3,
        MOUSE_OVER_SOUND_ON = 4,
        VIEWPORT_SOUND_ON = 5,
        VIEWPORT_SOUND_OFF = 6,
        CONTINUOUS = 7,
    }
}

ortb_code! {
    /// List: Pod Sequence.
    PodSequence(i8) {
        LAST = -1,
        ANY = 0,
        FIRST = 1,
    }
}

ortb_code! {
    /// List: Production Qualities.
    ProductionQuality(i8) {
        UNKNOWN = 0,
        PROFESSIONAL = 1,
        PROSUMER = 2,
        USER = 3,
    }
}

ortb_code! {
    /// List: Slot Position in Pod.
    SlotPositionInPod(i8) {
        LAST = -1,
        ANY = 0,
        FIRST = 1,
        FIRST_OR_LAST = 2,
    }
}

ortb_code! {
    /// List: Start Delay Modes. Positive values are the mid-roll offset in seconds.
    StartDelay(i64) {
        PRE_ROLL = 0,
        MID_ROLL = -1,
        POST_ROLL = -2,
    }
}

ortb_code! {
    /// List: User-Agent Source.
    UserAgentSource(i8) {
        UNKNOWN = 0,
        LOW_ENTROPY = 1,
        HIGH_ENTROPY = 2,
        PARSED = 3,
    }
}

ortb_code! {
    /// List: Placement Subtypes - Video (OpenRTB 2.5 `video.placement`, deprecated by `plcmt`).
    VideoPlacementSubtype(i8) {
        IN_STREAM = 1,
        IN_BANNER = 2,
        IN_ARTICLE = 3,
        IN_FEED = 4,
        ALWAYS_VISIBLE = 5,
    }
}

ortb_code! {
    /// List: Plcmt Subtypes - Video (OpenRTB 2.6 `video.plcmt`).
    VideoPlcmtSubtype(i8) {
        INSTREAM = 1,
        ACCOMPANYING_CONTENT = 2,
        INTERSTITIAL = 3,
        NO_CONTENT = 4,
    }
}

ortb_code! {
    /// List: Volume Normalization Modes.
    VolumeNormalizationMode(i8) {
        NONE = 0,
        AVERAGE = 1,
        PEAK = 2,
        LOUDNESS = 3,
        CUSTOM = 4,
    }
}
