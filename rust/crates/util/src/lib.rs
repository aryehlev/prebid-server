pub mod httputil;
pub mod iosutil;
pub mod iputil;
pub mod jsonutil;
pub mod maputil;
pub mod mathutil;
pub mod randomutil;
pub mod sliceutil;
pub mod stringutil;
pub mod timeutil;
pub mod uuidutil;

// Re-export commonly used items at the crate root for backward compatibility
pub use jsonutil::{deep_merge, get_f64, get_i64, get_string};
pub use iputil::{mask_ipv4, mask_ipv6, sanitize_ip};
pub use mathutil::truncate_decimal_places;
pub use sliceutil::{contains_string, random_shuffle, unique_strings};
pub use stringutil::truncate_utf8;
pub use uuidutil::new_uuid;

// Re-export HTTP utilities
pub use httputil::{encode_url_params, is_valid_url, parse_query_string_params};
