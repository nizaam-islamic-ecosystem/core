//! Trusted security context carried through engine execution.

/// Provider neutral security information available to downstream work.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SecurityContext;

impl SecurityContext {
    /// Creates an empty security context for trusted local execution.
    pub const fn new() -> Self {
        Self
    }
}
