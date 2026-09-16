use std::fmt;

/// Provider-neutral reference to a secret.
///
/// The reference identifies a secret without containing the resolved secret
/// value. Core deliberately treats the reference body as opaque.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecretReference {
    value: String,
}

impl SecretReference {
    const SCHEME: &'static str = "secret://";

    /// Creates a secret reference from a provider-neutral `secret://` value.
    pub fn new(value: impl Into<String>) -> Result<Self, SecretReferenceError> {
        let value = value.into();

        if value.is_empty() {
            return Err(SecretReferenceError::Empty);
        }

        if !value.starts_with(Self::SCHEME) {
            return Err(SecretReferenceError::InvalidScheme);
        }

        if value.len() == Self::SCHEME.len() {
            return Err(SecretReferenceError::EmptyReference);
        }

        Ok(Self { value })
    }

    /// Returns the complete reference string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns the opaque reference body after `secret://`.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.value[Self::SCHEME.len()..]
    }
}

/// A resolved secret value.
///
/// The value is intentionally not exposed through `Debug`. Callers must
/// explicitly request the underlying secret when they actually need to
/// consume it.
#[derive(Clone, Eq, PartialEq)]
pub struct SecretValue {
    value: String,
}

impl SecretValue {
    /// Creates a secret value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Explicitly exposes the secret to code that requires it.
    ///
    /// Callers are responsible for preventing disclosure through logs,
    /// diagnostics, metrics, traces, errors, or other output.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.value
    }

    /// Consumes the wrapper and returns the underlying secret.
    ///
    /// This operation is intentionally explicit because it removes the
    /// redaction boundary provided by `SecretValue`.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.value
    }

    /// Returns the length of the secret without exposing its contents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns whether the secret is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

/// Resolves provider-neutral secret references into sensitive secret values.
///
/// Core defines this interface but does not provide a concrete secret
/// provider.
pub trait SecretResolver {
    type Error;

    fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, Self::Error>;
}

/// Errors produced while constructing a secret reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretReferenceError {
    Empty,
    InvalidScheme,
    EmptyReference,
}

impl fmt::Display for SecretReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("secret reference must not be empty"),
            Self::InvalidScheme => f.write_str("secret reference must use the 'secret://' scheme"),
            Self::EmptyReference => f.write_str("secret reference body must not be empty"),
        }
    }
}

impl std::error::Error for SecretReferenceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_reference_is_accepted() {
        let reference =
            SecretReference::new("secret://database/password").expect("reference should be valid");

        assert_eq!(reference.as_str(), "secret://database/password");
        assert_eq!(reference.body(), "database/password");
    }

    #[test]
    fn empty_reference_is_rejected() {
        assert_eq!(
            SecretReference::new("").expect_err("empty reference must fail"),
            SecretReferenceError::Empty
        );
    }

    #[test]
    fn missing_scheme_is_rejected() {
        assert_eq!(
            SecretReference::new("database/password").expect_err("missing scheme must fail"),
            SecretReferenceError::InvalidScheme
        );
    }

    #[test]
    fn empty_reference_body_is_rejected() {
        assert_eq!(
            SecretReference::new("secret://").expect_err("empty reference body must fail"),
            SecretReferenceError::EmptyReference
        );
    }

    #[test]
    fn reference_body_remains_opaque() {
        let reference = SecretReference::new("secret://provider-specific/value")
            .expect("reference should be valid");

        assert_eq!(reference.body(), "provider-specific/value");
    }

    #[test]
    fn secret_value_can_be_created_and_explicitly_exposed() {
        let secret = SecretValue::new("test-secret");

        assert_eq!(secret.expose_secret(), "test-secret");
        assert_eq!(secret.len(), "test-secret".len());
        assert!(!secret.is_empty());
    }

    #[test]
    fn empty_secret_value_is_allowed() {
        let secret = SecretValue::new("");

        assert_eq!(secret.expose_secret(), "");
        assert_eq!(secret.len(), 0);
        assert!(secret.is_empty());
    }

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = SecretValue::new("super-secret-value");
        let rendered = format!("{secret:?}");

        assert_eq!(rendered, "SecretValue([REDACTED])");
        assert!(!rendered.contains("super-secret-value"));
    }

    #[test]
    fn secret_value_can_be_consumed_explicitly() {
        let secret = SecretValue::new("test-secret");

        assert_eq!(secret.into_inner(), "test-secret");
    }

    struct TestSecretResolver;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct TestResolverError;

    impl fmt::Display for TestResolverError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("test secret resolution failed")
        }
    }

    impl std::error::Error for TestResolverError {}

    impl SecretResolver for TestSecretResolver {
        type Error = TestResolverError;

        fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, Self::Error> {
            match reference.body() {
                "database/password" => Ok(SecretValue::new("resolved-secret")),
                _ => Err(TestResolverError),
            }
        }
    }

    #[test]
    fn custom_resolver_can_resolve_a_reference() {
        let reference =
            SecretReference::new("secret://database/password").expect("reference should be valid");

        let resolver = TestSecretResolver;
        let secret = resolver
            .resolve(&reference)
            .expect("resolver should succeed");

        assert_eq!(secret.expose_secret(), "resolved-secret");
    }

    #[test]
    fn custom_resolver_errors_are_preserved() {
        let reference =
            SecretReference::new("secret://missing").expect("reference should be valid");

        let resolver = TestSecretResolver;

        assert_eq!(
            resolver
                .resolve(&reference)
                .expect_err("resolution must fail"),
            TestResolverError
        );
    }

    #[test]
    fn reference_debug_output_contains_reference_not_secret_value() {
        let reference =
            SecretReference::new("secret://database/password").expect("reference should be valid");

        let rendered = format!("{reference:?}");

        assert!(rendered.contains("secret://database/password"));
        assert!(!rendered.contains("resolved-secret"));
    }

    #[test]
    fn secret_value_equality_is_value_based() {
        let first = SecretValue::new("same-secret");
        let second = SecretValue::new("same-secret");
        let different = SecretValue::new("different-secret");

        assert_eq!(first, second);
        assert_ne!(first, different);
    }

    #[test]
    fn reference_equality_is_reference_based() {
        let first =
            SecretReference::new("secret://database/password").expect("reference should be valid");
        let second =
            SecretReference::new("secret://database/password").expect("reference should be valid");
        let different =
            SecretReference::new("secret://api/key").expect("reference should be valid");

        assert_eq!(first, second);
        assert_ne!(first, different);
    }

    #[test]
    fn secret_reference_errors_have_safe_messages() {
        assert_eq!(
            SecretReferenceError::Empty.to_string(),
            "secret reference must not be empty"
        );
        assert_eq!(
            SecretReferenceError::InvalidScheme.to_string(),
            "secret reference must use the 'secret://' scheme"
        );
        assert_eq!(
            SecretReferenceError::EmptyReference.to_string(),
            "secret reference body must not be empty"
        );
    }
}
