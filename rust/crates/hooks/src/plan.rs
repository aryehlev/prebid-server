//! Execution plans for hooks.
//!
//! A plan is a list of [`Group`]s, each of which in turn contains a list of
//! [`crate::hook::HookWrapper`]s that should run in parallel (subject to a
//! per-group timeout).

use std::time::Duration;

use crate::hook::HookWrapper;

/// A group of hooks that runs together under a shared timeout.
#[derive(Clone)]
pub struct Group<P>
where
    P: Send + 'static,
{
    /// The max duration allowed for all hooks in the group.
    pub timeout: Duration,
    /// The hooks composing the group.
    pub hooks: Vec<HookWrapper<P>>,
}

impl<P: Send + 'static> Group<P> {
    /// Construct a new group.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            hooks: Vec::new(),
        }
    }

    /// Append a hook to this group.
    pub fn push(&mut self, hook: HookWrapper<P>) -> &mut Self {
        self.hooks.push(hook);
        self
    }
}

/// A full execution plan for a single stage: an ordered list of groups.
#[derive(Clone)]
pub struct HookExecutionPlan<P>
where
    P: Send + 'static,
{
    /// Ordered groups to execute.
    pub groups: Vec<Group<P>>,
}

impl<P: Send + 'static> Default for HookExecutionPlan<P> {
    fn default() -> Self {
        Self { groups: Vec::new() }
    }
}

impl<P: Send + 'static> HookExecutionPlan<P> {
    /// Construct an empty plan.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a group to the plan.
    pub fn push_group(&mut self, group: Group<P>) -> &mut Self {
        self.groups.push(group);
        self
    }

    /// Returns `true` if the plan has no groups.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Total number of hooks across all groups.
    pub fn len(&self) -> usize {
        self.groups.iter().map(|g| g.hooks.len()).sum()
    }
}
