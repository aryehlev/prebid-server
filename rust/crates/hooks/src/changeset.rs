//! Payload mutations recorded by hooks.
//!
//! Corresponds to `hooks/hookstage/mutation.go` in the Go code base.

use std::fmt;
use std::sync::Arc;

use crate::reject::HookError;

/// The kind of mutation that a hook intends to apply to the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    /// Insert a new field into the payload.
    Add,
    /// Replace an existing field's value.
    Update,
    /// Remove an existing field from the payload.
    Delete,
}

impl fmt::Display for MutationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MutationType::Add => "add",
            MutationType::Update => "update",
            MutationType::Delete => "delete",
        })
    }
}

/// Type alias for a function that knows how to apply a mutation to a payload.
pub type MutationFn<T> = Arc<dyn Fn(T) -> Result<T, HookError> + Send + Sync>;

/// A single mutation to be applied to a payload of type `T`.
pub struct Mutation<T> {
    mut_type: MutationType,
    key: Vec<String>,
    func: MutationFn<T>,
}

impl<T> Clone for Mutation<T> {
    fn clone(&self) -> Self {
        Self {
            mut_type: self.mut_type,
            key: self.key.clone(),
            func: self.func.clone(),
        }
    }
}

impl<T> Mutation<T> {
    /// Create a new mutation from its component parts.
    pub fn new<F>(mut_type: MutationType, key: Vec<String>, func: F) -> Self
    where
        F: Fn(T) -> Result<T, HookError> + Send + Sync + 'static,
    {
        Self {
            mut_type,
            key,
            func: Arc::new(func),
        }
    }

    /// The type (add / update / delete) of this mutation.
    pub fn mutation_type(&self) -> MutationType {
        self.mut_type
    }

    /// The dotted path segments describing the affected field.
    pub fn key(&self) -> &[String] {
        &self.key
    }

    /// Apply the mutation to a payload, returning the transformed payload.
    pub fn apply(&self, payload: T) -> Result<T, HookError> {
        (self.func)(payload)
    }
}

impl<T> fmt::Debug for Mutation<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutation")
            .field("type", &self.mut_type)
            .field("key", &self.key)
            .finish()
    }
}

/// Aggregated set of mutations a hook wishes to apply.
pub struct ChangeSet<T> {
    mutations: Vec<Mutation<T>>,
}

impl<T> Default for ChangeSet<T> {
    fn default() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }
}

impl<T> ChangeSet<T> {
    /// Construct an empty change set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The slice of mutations recorded on this change set.
    pub fn mutations(&self) -> &[Mutation<T>] {
        &self.mutations
    }

    /// Number of recorded mutations.
    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    /// Whether the change set contains no mutations.
    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Append a mutation, returning `&mut self` for builder-style chaining.
    pub fn add_mutation<F>(
        &mut self,
        func: F,
        mut_type: MutationType,
        key: Vec<String>,
    ) -> &mut Self
    where
        F: Fn(T) -> Result<T, HookError> + Send + Sync + 'static,
    {
        self.mutations
            .push(Mutation::new(mut_type, key, func));
        self
    }

    /// Append a pre-constructed mutation.
    pub fn push(&mut self, m: Mutation<T>) -> &mut Self {
        self.mutations.push(m);
        self
    }

    /// Apply every mutation in order to the provided payload, collecting any
    /// errors. On a mutation error, the payload is restored to its state
    /// before that mutation via `Clone` and iteration continues.
    pub fn apply_all(&self, payload: T) -> (T, Vec<HookError>)
    where
        T: Clone,
    {
        let mut current = payload;
        let mut errors = Vec::new();
        for m in &self.mutations {
            let backup = current.clone();
            match m.apply(current) {
                Ok(updated) => current = updated,
                Err(e) => {
                    errors.push(e);
                    current = backup;
                }
            }
        }
        (current, errors)
    }
}

impl<T> Clone for ChangeSet<T> {
    fn clone(&self) -> Self {
        Self {
            mutations: self.mutations.clone(),
        }
    }
}

impl<T> fmt::Debug for ChangeSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangeSet")
            .field("mutations", &self.mutations)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_apply_mutation() {
        let mut cs: ChangeSet<i32> = ChangeSet::new();
        cs.add_mutation(
            |v| Ok(v + 1),
            MutationType::Update,
            vec!["value".to_string()],
        );
        cs.add_mutation(
            |v| Ok(v * 2),
            MutationType::Update,
            vec!["value".to_string()],
        );
        assert_eq!(cs.len(), 2);
        let (out, errs) = cs.apply_all(3);
        assert!(errs.is_empty());
        assert_eq!(out, 8); // (3+1)*2
    }

    #[test]
    fn mutation_type_display() {
        assert_eq!(MutationType::Add.to_string(), "add");
        assert_eq!(MutationType::Update.to_string(), "update");
        assert_eq!(MutationType::Delete.to_string(), "delete");
    }
}
