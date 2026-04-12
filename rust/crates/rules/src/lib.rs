//! Rust port of the Go `rules/` package — a generic decision-tree rules engine.
//!
//! A [`Tree`] is composed of [`Node`]s. Each internal node holds a
//! [`SchemaFunction`] that is invoked against a payload/context; the function's
//! string result is then used as a key to select the next child node. Leaf
//! nodes carry a list of [`ResultFunction`]s that mutate an output context.
//!
//! Mirrors Go files:
//!   - rules/tree.go
//!   - rules/schema_functions.go
//!   - rules/result_functions.go

pub mod schema_functions;
pub mod result_functions;

use std::collections::HashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum RulesError {
    #[error("tree root is nil")]
    NilRoot,
    #[error("schema function is nil")]
    NilSchemaFunction,
    #[error("tree is malformed: leaves found at different depths")]
    Unbalanced,
    #[error("schema function error: {0}")]
    SchemaFunction(String),
    #[error("result function error: {0}")]
    ResultFunction(String),
}

// ---------------------------------------------------------------------------
// Function traits
// ---------------------------------------------------------------------------

/// A [`SchemaFunction`] is invoked against the input context and returns a
/// string value which is compared against child-node keys when traversing a
/// [`Tree`].
pub trait SchemaFunction<Ctx>: Send + Sync {
    /// Name of the schema function (used for analytics/logging).
    fn name(&self) -> &str;
    /// Evaluate the function against the provided context.
    fn call(&self, ctx: &Ctx) -> Result<String, RulesError>;
}

/// A [`ResultFunction`] mutates an output context when a matching leaf is
/// reached (or as a default action).
pub trait ResultFunction<Ctx, Out>: Send + Sync {
    /// Name of the result function.
    fn name(&self) -> &str;
    /// Apply the result function to the output context.
    fn call(
        &self,
        payload: &Ctx,
        out: &mut Out,
        meta: &ResultFunctionMeta,
    ) -> Result<(), RulesError>;
}

// ---------------------------------------------------------------------------
// Analytics/trace metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct SchemaFunctionStep {
    pub func_name: String,
    pub func_result: String,
}

#[derive(Debug, Default, Clone)]
pub struct ResultFunctionMeta {
    pub schema_function_results: Vec<SchemaFunctionStep>,
    pub analytics_key: String,
    pub rule_fired: String,
    pub model_version: String,
}

impl ResultFunctionMeta {
    fn append_schema_result(&mut self, name: &str, value: &str) {
        self.schema_function_results.push(SchemaFunctionStep {
            func_name: name.to_string(),
            func_result: value.to_string(),
        });
    }

    fn append_rule_fired(&mut self, value: &str) {
        if self.rule_fired.is_empty() {
            self.rule_fired = value.to_string();
        } else {
            self.rule_fired.push('|');
            self.rule_fired.push_str(value);
        }
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// A node in the decision [`Tree`]. Internal nodes have a schema function and
/// children; leaf nodes have a list of result functions.
pub struct Node<Ctx, Out> {
    pub schema_function: Option<Box<dyn SchemaFunction<Ctx>>>,
    pub result_functions: Vec<Box<dyn ResultFunction<Ctx, Out>>>,
    pub children: HashMap<String, Node<Ctx, Out>>,
}

impl<Ctx, Out> Default for Node<Ctx, Out> {
    fn default() -> Self {
        Self {
            schema_function: None,
            result_functions: Vec::new(),
            children: HashMap::new(),
        }
    }
}

impl<Ctx, Out> Node<Ctx, Out> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the node has no children.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Select the child matching `value`, falling back to a wildcard `"*"`
    /// child if no exact match exists. Returns the matched key and a reference
    /// to the child.
    pub fn match_child(&self, value: &str) -> Option<(String, &Node<Ctx, Out>)> {
        if let Some(child) = self.children.get(value) {
            return Some((value.to_string(), child));
        }
        if let Some(child) = self.children.get("*") {
            return Some(("*".to_string(), child));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/// A decision tree over an input context `Ctx` producing side effects on an
/// output context `Out`.
pub struct Tree<Ctx, Out> {
    pub root: Option<Node<Ctx, Out>>,
    pub default_functions: Vec<Box<dyn ResultFunction<Ctx, Out>>>,
    pub analytics_key: String,
    pub model_version: String,
}

impl<Ctx, Out> Default for Tree<Ctx, Out> {
    fn default() -> Self {
        Self {
            root: None,
            default_functions: Vec::new(),
            analytics_key: String::new(),
            model_version: String::new(),
        }
    }
}

impl<Ctx, Out> Tree<Ctx, Out> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the tree: all leaves must be at the same depth.
    pub fn validate(&self) -> Result<(), RulesError> {
        let Some(root) = self.root.as_ref() else {
            return Ok(());
        };
        let mut first_leaf_depth: i32 = -1;
        if !validate_node(root, 0, &mut first_leaf_depth) {
            return Err(RulesError::Unbalanced);
        }
        Ok(())
    }

    /// Walk the tree by repeatedly calling the current node's schema function
    /// and taking the child whose key matches the function's output. When a
    /// leaf is reached its result functions are executed; otherwise
    /// `default_functions` are invoked.
    pub fn run(&self, payload: &Ctx, out: &mut Out) -> Result<ResultFunctionMeta, RulesError> {
        let root = self.root.as_ref().ok_or(RulesError::NilRoot)?;
        let mut curr: Option<&Node<Ctx, Out>> = Some(root);

        let mut meta = ResultFunctionMeta {
            analytics_key: self.analytics_key.clone(),
            model_version: self.model_version.clone(),
            ..Default::default()
        };

        while let Some(node) = curr {
            if node.is_leaf() {
                break;
            }
            let sf = node
                .schema_function
                .as_ref()
                .ok_or(RulesError::NilSchemaFunction)?;
            let res = sf.call(payload)?;
            meta.append_schema_result(sf.name(), &res);

            match node.match_child(&res) {
                Some((key, child)) => {
                    meta.append_rule_fired(&key);
                    curr = Some(child);
                }
                None => {
                    meta.rule_fired = "default".to_string();
                    curr = None;
                    break;
                }
            }
        }

        let funcs: &Vec<Box<dyn ResultFunction<Ctx, Out>>> = match curr {
            Some(node) => &node.result_functions,
            None => &self.default_functions,
        };
        for rf in funcs {
            rf.call(payload, out, &meta)?;
        }
        Ok(meta)
    }
}

fn validate_node<Ctx, Out>(
    node: &Node<Ctx, Out>,
    depth: i32,
    first_leaf_depth: &mut i32,
) -> bool {
    if node.is_leaf() {
        if *first_leaf_depth == -1 {
            *first_leaf_depth = depth;
            return true;
        }
        if depth != *first_leaf_depth {
            return false;
        }
    }
    for child in node.children.values() {
        if !validate_node(child, depth + 1, first_leaf_depth) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Rules facade — a thin wrapper providing an `evaluate` entry point.
// ---------------------------------------------------------------------------

/// High-level convenience wrapper around a [`Tree`].
pub struct Rules<Ctx, Out> {
    pub tree: Tree<Ctx, Out>,
}

impl<Ctx, Out> Rules<Ctx, Out> {
    pub fn new(tree: Tree<Ctx, Out>) -> Self {
        Self { tree }
    }

    /// Evaluate the rules tree against `ctx`, mutating `out`.
    pub fn evaluate(&self, ctx: &Ctx, out: &mut Out) -> Result<ResultFunctionMeta, RulesError> {
        self.tree.run(ctx, out)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result_functions::{ExcludeBidders, IncludeBidders, BidderCtx};
    use crate::schema_functions::{Channel, DeviceCountry, DeviceType, RequestCtx};

    #[test]
    fn test_is_leaf() {
        let n: Node<(), ()> = Node::new();
        assert!(n.is_leaf());

        let mut n2: Node<(), ()> = Node::new();
        n2.children.insert("a".into(), Node::new());
        assert!(!n2.is_leaf());
    }

    #[test]
    fn test_match_child_exact_and_wildcard() {
        let mut n: Node<(), ()> = Node::new();
        n.children.insert("child-two".into(), Node::new());
        n.children.insert("*".into(), Node::new());

        let (key, _) = n.match_child("child-two").unwrap();
        assert_eq!(key, "child-two");

        let (key, _) = n.match_child("nope").unwrap();
        assert_eq!(key, "*");

        let mut n2: Node<(), ()> = Node::new();
        n2.children.insert("only".into(), Node::new());
        assert!(n2.match_child("missing").is_none());
    }

    #[test]
    fn test_validate_nil_root() {
        let t: Tree<(), ()> = Tree::new();
        assert!(t.validate().is_ok());
    }

    #[test]
    fn test_validate_balanced() {
        let mut root: Node<(), ()> = Node::new();
        let mut a = Node::new();
        a.children.insert("x".into(), Node::new());
        a.children.insert("y".into(), Node::new());
        let mut b = Node::new();
        b.children.insert("x".into(), Node::new());
        root.children.insert("a".into(), a);
        root.children.insert("b".into(), b);
        let t: Tree<(), ()> = Tree {
            root: Some(root),
            ..Default::default()
        };
        assert!(t.validate().is_ok());
    }

    #[test]
    fn test_validate_unbalanced() {
        let mut root: Node<(), ()> = Node::new();
        let mut a = Node::new();
        a.children.insert("x".into(), Node::new()); // leaf at depth 2
        root.children.insert("a".into(), a);
        root.children.insert("b".into(), Node::new()); // leaf at depth 1
        let t: Tree<(), ()> = Tree {
            root: Some(root),
            ..Default::default()
        };
        assert!(matches!(t.validate(), Err(RulesError::Unbalanced)));
    }

    #[test]
    fn test_tree_run_matches_leaf_and_excludes_bidder() {
        // Build: root -> deviceCountry -> { "US": leaf[exclude(appnexus)], "*": leaf[include(rubicon)] }
        let mut leaf_us: Node<RequestCtx, BidderCtx> = Node::new();
        leaf_us.result_functions.push(Box::new(ExcludeBidders {
            bidders: vec!["appnexus".to_string()],
        }));

        let mut leaf_other: Node<RequestCtx, BidderCtx> = Node::new();
        leaf_other.result_functions.push(Box::new(IncludeBidders {
            bidders: vec!["rubicon".to_string()],
        }));

        let mut root: Node<RequestCtx, BidderCtx> = Node::new();
        root.schema_function = Some(Box::new(DeviceCountry));
        root.children.insert("US".into(), leaf_us);
        root.children.insert("*".into(), leaf_other);

        let tree = Tree {
            root: Some(root),
            analytics_key: "test".into(),
            model_version: "v1".into(),
            ..Default::default()
        };
        tree.validate().unwrap();

        let rules = Rules::new(tree);

        // Case 1: US -> exclude appnexus
        let ctx = RequestCtx {
            device_country: "US".into(),
            channel: "web".into(),
            device_type: "phone".into(),
        };
        let mut out = BidderCtx::default();
        let meta = rules.evaluate(&ctx, &mut out).unwrap();
        assert_eq!(meta.rule_fired, "US");
        assert_eq!(out.excluded_bidders, vec!["appnexus".to_string()]);
        assert!(out.included_bidders.is_empty());

        // Case 2: FR -> wildcard -> include rubicon
        let ctx2 = RequestCtx {
            device_country: "FR".into(),
            channel: "app".into(),
            device_type: "tablet".into(),
        };
        let mut out2 = BidderCtx::default();
        let meta2 = rules.evaluate(&ctx2, &mut out2).unwrap();
        assert_eq!(meta2.rule_fired, "*");
        assert_eq!(out2.included_bidders, vec!["rubicon".to_string()]);
    }

    #[test]
    fn test_tree_run_no_match_runs_default_functions() {
        let mut root: Node<RequestCtx, BidderCtx> = Node::new();
        root.schema_function = Some(Box::new(Channel));
        root.children.insert("web".into(), Node::new()); // leaf

        let tree = Tree {
            root: Some(root),
            default_functions: vec![Box::new(ExcludeBidders {
                bidders: vec!["default_bidder".to_string()],
            })],
            ..Default::default()
        };
        let rules = Rules::new(tree);

        let ctx = RequestCtx {
            device_country: "".into(),
            channel: "app".into(), // not "web"
            device_type: "".into(),
        };
        let mut out = BidderCtx::default();
        let meta = rules.evaluate(&ctx, &mut out).unwrap();
        assert_eq!(meta.rule_fired, "default");
        assert_eq!(out.excluded_bidders, vec!["default_bidder".to_string()]);
    }

    #[test]
    fn test_tree_run_nil_root_errors() {
        let tree: Tree<RequestCtx, BidderCtx> = Tree::new();
        let rules = Rules::new(tree);
        let ctx = RequestCtx::default();
        let mut out = BidderCtx::default();
        assert!(matches!(
            rules.evaluate(&ctx, &mut out),
            Err(RulesError::NilRoot)
        ));
    }

    #[test]
    fn test_device_type_schema_function() {
        let sf = DeviceType;
        let ctx = RequestCtx {
            device_type: "phone".into(),
            ..Default::default()
        };
        assert_eq!(sf.call(&ctx).unwrap(), "phone");
        assert_eq!(sf.name(), "deviceType");
    }
}
