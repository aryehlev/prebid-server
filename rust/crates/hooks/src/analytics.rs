//! Analytics tag types emitted by hooks.
//!
//! Corresponds to `hooks/hookanalytics/analytics.go`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The application status of an analytics result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appliable {
    /// The hook's suggestion was applied to the request/response.
    SuccessApplied,
    /// The hook succeeded but the change was not applied.
    SuccessNotApplied,
    /// The hook produced an error.
    Error,
}

impl Default for Appliable {
    fn default() -> Self {
        Appliable::SuccessApplied
    }
}

/// A single analytics result emitted by a hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalyticsResult {
    /// The status of this result.
    pub status: Option<Appliable>,
    /// Free-form analytics values.
    #[serde(default)]
    pub values: Value,
    /// The module-specific label for this result.
    #[serde(default)]
    pub app_activities: Vec<String>,
}

/// A named analytics activity, produced by a hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Activity {
    /// The activity name.
    pub name: String,
    /// The activity status.
    pub status: Option<Appliable>,
    /// The set of `AnalyticsResult`s belonging to the activity.
    #[serde(default)]
    pub results: Vec<AnalyticsResult>,
}

/// The top-level analytics tag container emitted by a hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalyticsTags {
    /// The list of activities reported by the hook.
    #[serde(default)]
    pub activities: Vec<Activity>,
}
