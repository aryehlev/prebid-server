//! UUID utilities — mirrors Go `util/uuidutil` package.

use uuid::Uuid;

/// UUIDGenerator trait for testability — mirrors Go `uuidutil.UUIDGenerator`.
pub trait UuidGenerator: Send + Sync {
    fn generate(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}

/// Default UUID generator using UUID v4.
pub struct UuidRandomGenerator;

impl UuidGenerator for UuidRandomGenerator {
    fn generate(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Uuid::new_v4().to_string())
    }
}

/// Generate a new UUID v4 string.
pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}
