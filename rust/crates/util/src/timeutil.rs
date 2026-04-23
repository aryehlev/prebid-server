//! Time utilities — mirrors Go `util/timeutil` package.
//! Provides a testable time abstraction.

use chrono::{DateTime, Utc};

/// Time trait for testability — mirrors Go `timeutil.Time` interface.
pub trait Time: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// RealTime uses the system clock.
pub struct RealTime;

impl Time for RealTime {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
