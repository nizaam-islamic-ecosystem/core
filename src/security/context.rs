/// Provider neutral security information available to downstream work.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SecurityContext;

impl SecurityContext {
    /// Creates an empty security context for trusted local execution.
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::SecurityContext;

    #[test]
    fn security_context_is_cloneable_and_provider_neutral() {
        let context = SecurityContext::new();

        assert_eq!(context, context.clone());
    }
}
