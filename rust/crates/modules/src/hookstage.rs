//! Hook stage definitions and mutation system.
//!
//! Mirrors Go `hooks/hookstage/` — defines payload types, hook result,
//! mutation/changeset system, and module invocation context.

use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// MutationType — mirrors Go hookstage/mutation.go
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    Add,
    Update,
    Delete,
}

impl std::fmt::Display for MutationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutationType::Add => write!(f, "add"),
            MutationType::Update => write!(f, "update"),
            MutationType::Delete => write!(f, "delete"),
        }
    }
}

// ---------------------------------------------------------------------------
// Mutation and ChangeSet — mirrors Go hookstage/mutation.go
// ---------------------------------------------------------------------------

/// A mutation function that transforms a payload.
/// On error, returns (original_value, error_message) so the payload is not lost.
pub type MutationFunc<T> = Box<dyn FnOnce(T) -> Result<T, (T, String)> + Send>;

/// Represents a single mutation to apply to a payload.
pub struct Mutation<T> {
    pub mut_type: MutationType,
    pub key: Vec<String>,
    func_: MutationFunc<T>,
}

impl<T> Mutation<T> {
    pub fn apply(self, payload: T) -> Result<T, (T, String)> {
        (self.func_)(payload)
    }
}

/// A set of mutations to apply to a hook payload.
pub struct ChangeSet<T> {
    mutations: Vec<Mutation<T>>,
}

impl<T> Default for ChangeSet<T> {
    fn default() -> Self {
        Self { mutations: Vec::new() }
    }
}

impl<T> ChangeSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mutation(&mut self, func_: MutationFunc<T>, mut_type: MutationType, key: Vec<String>) {
        self.mutations.push(Mutation { mut_type, key, func_ });
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn drain(&mut self) -> Vec<Mutation<T>> {
        std::mem::take(&mut self.mutations)
    }
}

// ---------------------------------------------------------------------------
// ModuleContext — mirrors Go hookstage/invocation.go
// ---------------------------------------------------------------------------

/// Arbitrary data passed between module hooks at different stages.
pub type ModuleContext = HashMap<String, Value>;

/// Data passed to a module hook during invocation.
#[derive(Debug, Clone, Default)]
pub struct ModuleInvocationContext {
    pub account_id: String,
    pub account_config: Option<Value>,
    pub endpoint: String,
    pub module_context: ModuleContext,
    pub hook_impl_code: String,
}

// ---------------------------------------------------------------------------
// HookResult — mirrors Go hookstage/invocation.go
// ---------------------------------------------------------------------------

/// Result of executing a hook instance.
pub struct HookResult<T> {
    pub reject: bool,
    pub nbr_code: i32,
    pub message: String,
    pub change_set: ChangeSet<T>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub debug_messages: Vec<String>,
    pub module_context: ModuleContext,
}

impl<T> Default for HookResult<T> {
    fn default() -> Self {
        Self {
            reject: false,
            nbr_code: 0,
            message: String::new(),
            change_set: ChangeSet::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            debug_messages: Vec::new(),
            module_context: ModuleContext::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Payload types — mirrors Go hookstage/*.go
// ---------------------------------------------------------------------------

/// Entrypoint stage payload: raw HTTP body.
#[derive(Debug, Clone, Default)]
pub struct EntrypointPayload {
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

/// Raw auction request payload: raw body bytes.
pub type RawAuctionRequestPayload = Vec<u8>;

/// Bidder request payload: a bid request for a specific bidder.
#[derive(Debug, Clone, Default)]
pub struct BidderRequestPayload {
    pub request: Value,
    pub bidder: String,
}

/// Raw bidder response payload: response from a specific bidder.
#[derive(Debug, Clone, Default)]
pub struct RawBidderResponsePayload {
    pub response: Value,
    pub bidder: String,
}

/// All processed bid responses payload.
#[derive(Debug, Clone, Default)]
pub struct AllProcessedBidResponsesPayload {
    pub responses: HashMap<String, Value>,
}

/// Auction response payload: final response to send back.
#[derive(Debug, Clone, Default)]
pub struct AuctionResponsePayload {
    pub response: Value,
}

/// Exitpoint payload: response + headers.
#[derive(Debug, Clone, Default)]
pub struct ExitpointPayload {
    pub response: Value,
    pub headers: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Hook traits — mirrors Go hookstage interfaces
// ---------------------------------------------------------------------------

/// Entrypoint hook trait.
#[async_trait::async_trait]
pub trait EntrypointHook: Send + Sync {
    async fn handle_entrypoint_hook(
        &self,
        ctx: ModuleInvocationContext,
        payload: EntrypointPayload,
    ) -> Result<HookResult<EntrypointPayload>, String>;
}

/// Bidder request hook trait.
#[async_trait::async_trait]
pub trait BidderRequestHook: Send + Sync {
    async fn handle_bidder_request_hook(
        &self,
        ctx: ModuleInvocationContext,
        payload: BidderRequestPayload,
    ) -> Result<HookResult<BidderRequestPayload>, String>;
}

/// Raw bidder response hook trait.
#[async_trait::async_trait]
pub trait RawBidderResponseHook: Send + Sync {
    async fn handle_raw_bidder_response_hook(
        &self,
        ctx: ModuleInvocationContext,
        payload: RawBidderResponsePayload,
    ) -> Result<HookResult<RawBidderResponsePayload>, String>;
}

/// Auction response hook trait.
#[async_trait::async_trait]
pub trait AuctionResponseHook: Send + Sync {
    async fn handle_auction_response_hook(
        &self,
        ctx: ModuleInvocationContext,
        payload: AuctionResponsePayload,
    ) -> Result<HookResult<AuctionResponsePayload>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_type_display() {
        assert_eq!(MutationType::Add.to_string(), "add");
        assert_eq!(MutationType::Update.to_string(), "update");
        assert_eq!(MutationType::Delete.to_string(), "delete");
    }

    #[test]
    fn test_changeset_add_mutation() {
        let mut cs: ChangeSet<String> = ChangeSet::new();
        assert!(cs.is_empty());
        cs.add_mutation(
            Box::new(|s: String| Ok(format!("{}_modified", s))),
            MutationType::Update,
            vec!["test".to_string()],
        );
        assert!(!cs.is_empty());

        let mutations = cs.drain();
        assert_eq!(mutations.len(), 1);
        let result = mutations.into_iter().next().unwrap().apply("hello".to_string()).unwrap();
        assert_eq!(result, "hello_modified");
    }

    #[test]
    fn test_hook_result_default() {
        let result: HookResult<String> = HookResult::default();
        assert!(!result.reject);
        assert_eq!(result.nbr_code, 0);
        assert!(result.errors.is_empty());
    }
}
