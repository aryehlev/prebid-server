//! OpenRTB Dynamic Native Ads 1.2 (Go `github.com/risecodes/openrtb/native1`).
//!
//! [`request::Request`] is what `imp.native.request` holds as a JSON string;
//! [`response::Response`] is the native `adm` of a bid.

pub mod request;
pub mod response;

use super::de::ortb_code;

ortb_code! {
    /// Native Ad Unit IDs (deprecated in 1.2).
    AdUnit(i64) {
        PAID_SEARCH = 1,
        RECOMMENDATION_WIDGET = 2,
        PROMOTED_LISTING = 3,
        IN_AD = 4,
        CUSTOM = 5,
    }
}

ortb_code! {
    /// Context Sub Type IDs.
    ContextSubType(i64) {
        GENERAL = 10,
        ARTICLE = 11,
        VIDEO = 12,
        AUDIO = 13,
        IMAGE = 14,
        USER_GENERATED = 15,
        SOCIAL = 20,
        EMAIL = 21,
        CHAT = 22,
        SELLING = 30,
        APP_STORE = 31,
        PRODUCT_REVIEW = 32,
    }
}

ortb_code! {
    /// Context Type IDs.
    ContextType(i64) {
        CONTENT = 1,
        SOCIAL = 2,
        PRODUCT = 3,
    }
}

ortb_code! {
    /// Data Asset Types.
    DataAssetType(i64) {
        SPONSORED = 1,
        DESC = 2,
        RATING = 3,
        LIKES = 4,
        DOWNLOADS = 5,
        PRICE = 6,
        SALE_PRICE = 7,
        PHONE = 8,
        ADDRESS = 9,
        DESC2 = 10,
        DISPLAY_URL = 11,
        CTA_TEXT = 12,
    }
}

ortb_code! {
    /// Event Tracking Methods.
    EventTrackingMethod(i64) {
        IMAGE = 1,
        JS = 2,
    }
}

ortb_code! {
    /// Event Types.
    EventType(i64) {
        IMPRESSION = 1,
        VIEWABLE_MRC50 = 2,
        VIEWABLE_MRC100 = 3,
        VIEWABLE_VIDEO50 = 4,
    }
}

ortb_code! {
    /// Image Asset Types.
    ImageAssetType(i64) {
        ICON = 1,
        LOGO = 2,
        MAIN = 3,
    }
}

ortb_code! {
    /// Native Layout IDs (deprecated in 1.2).
    Layout(i64) {
        CONTENT_WALL = 1,
        APP_WALL = 2,
        NEWS_FEED = 3,
        CHAT_LIST = 4,
        CAROUSEL = 5,
        CONTENT_STREAM = 6,
        GRID = 7,
    }
}

ortb_code! {
    /// Placement Type IDs.
    PlacementType(i64) {
        FEED = 1,
        ATOMIC_CONTENT_UNIT = 2,
        OUTSIDE_CORE_CONTENT = 3,
        RECOMMENDATION_WIDGET = 4,
    }
}
