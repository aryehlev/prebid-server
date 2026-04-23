//! Rust port of the Prebid Server Go `hooks` package.
//!
//! This crate provides the abstractions needed to define, register, and
//! execute "module hooks" at various stages of the auction pipeline.
//!
//! The API is heavily inspired by the Go implementation found under
//! `hooks/hookstage`, `hooks/hookexecution`, `hooks/plan.go` and
//! `hooks/config.go` in the upstream repository, but it is recast in
//! idiomatic asynchronous Rust on top of `tokio`.

pub mod analytics;
pub mod changeset;
pub mod executor;
pub mod hook;
pub mod module;
pub mod plan;
pub mod reject;
pub mod stage;

pub use analytics::{Activity, AnalyticsResult, AnalyticsTags, Appliable};
pub use changeset::{ChangeSet, Mutation, MutationFn, MutationType};
pub use executor::{HookExecutor, StageExecutionSummary};
pub use hook::{Hook, HookHandle, HookResult, HookWrapper, ModuleContext, ModuleInvocationContext};
pub use module::{Module, ModuleBuilder};
pub use plan::{Group, HookExecutionPlan};
pub use reject::{HookError, Reject};
pub use stage::Stage;
