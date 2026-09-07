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

    #[test]
    fn empty_context_returns_none_for_missing_keys() {
        let context = ProvenanceContext::new();
        assert_eq!(context.attribute("missing"), None);
    }

    #[test]
    fn default_context_equals_new() {
        assert_eq!(ProvenanceContext::default(), ProvenanceContext::new());
    }

    #[test]
    fn with_attribute_overrides_existing_value() {
        let context = ProvenanceContext::new()
            .with_attribute("key", "original")
            .with_attribute("key", "updated");

        assert_eq!(context.attribute("key"), Some("updated"));
    }

    #[test]
    fn with_attribute_accepts_string_and_str_inputs() {
        let context = ProvenanceContext::new()
            .with_attribute("string", String::from("value"))
            .with_attribute("str", "value");

        assert_eq!(context.attribute("string"), Some("value"));
        assert_eq!(context.attribute("str"), Some("value"));
    }

    #[test]
    fn provenance_context_supports_clone_and_eq() {
        let original = ProvenanceContext::new().with_attribute("k", "v");
        let clone = original.clone();

        assert_eq!(original, clone);
    }
}
