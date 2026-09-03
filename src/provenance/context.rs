use std::{collections::BTreeMap, sync::Arc};

/// Provider neutral provenance information available to downstream work.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProvenanceContext {
    attributes: Arc<BTreeMap<String, String>>,
}

impl ProvenanceContext {
    /// Creates an empty provenance context for trusted local execution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a derived context with one immutable provenance attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.attributes).insert(key.into(), value.into());
        self
    }

    /// Reads a provenance attribute from this context.
    pub fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::ProvenanceContext;

    #[test]
    fn derived_context_preserves_shared_attributes() {
        let context = ProvenanceContext::new().with_attribute("source", "fixture");
        let derived = context.clone().with_attribute("stage", "decode");

        assert_eq!(context.attribute("source"), Some("fixture"));
        assert_eq!(context.attribute("stage"), None);
        assert_eq!(derived.attribute("stage"), Some("decode"));
    }
}
