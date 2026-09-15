//! Integrity information and verification for artifact content.
//!
//! Integrity answers: "Are these bytes the expected bytes?"
//!
//! Phase 10 uses SHA-256 as the artifact content digest algorithm.
//!
//! Generic structural validation answers: "Does this artifact satisfy the
//! generic structural/contract requirements expected by Core?"
//!
//! Domain validation answers: "Does this artifact satisfy engine/domain-specific
//! semantic requirements?" Core MUST NOT provide domain-specific validation.

use sha2::{Digest, Sha256};

/// SHA-256 digest length in bytes.
pub const SHA256_DIGEST_LENGTH: usize = 32;

/// SHA-256 digest of artifact content.
///
/// The digest identifies the exact content representation used for integrity
/// verification. It does not replace `ArtifactId` or version identity.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentDigest {
    digest_bytes: [u8; SHA256_DIGEST_LENGTH],
}

impl ContentDigest {
    /// Computes the SHA-256 digest of the supplied content.
    pub fn new(content: &[u8]) -> Self {
        let digest = Sha256::digest(content);

        let mut digest_bytes = [0u8; SHA256_DIGEST_LENGTH];
        digest_bytes.copy_from_slice(&digest);

        Self { digest_bytes }
    }

    /// Constructs a digest from already computed SHA-256 digest bytes.
    pub fn from_bytes(digest_bytes: [u8; SHA256_DIGEST_LENGTH]) -> Self {
        Self { digest_bytes }
    }

    /// Returns the raw SHA-256 digest bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.digest_bytes
    }

    /// Returns the SHA-256 digest as lowercase hexadecimal text.
    pub fn to_hex(&self) -> String {
        self.digest_bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Returns the algorithm identifier used for this digest.
    pub const fn algorithm() -> &'static str {
        "sha-256"
    }
}

/// Evidence that concrete content was verified against an expected digest.
///
/// The fields are private so callers cannot construct a proof by supplying an
/// arbitrary digest or size. A proof can only be created through
/// [`IntegrityProof::verify`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityProof {
    digest: ContentDigest,
    size: u64,
}

impl IntegrityProof {
    /// Verifies concrete content against the expected digest and records the
    /// content size as part of the proof.
    pub fn verify(content: &[u8], expected: &ContentDigest) -> Result<Self, IntegrityError> {
        let actual = ContentDigest::new(content);

        if actual != *expected {
            return Err(IntegrityError::new(expected.clone(), actual));
        }

        Ok(Self {
            digest: actual,
            size: content.len() as u64,
        })
    }

    /// Returns the digest established by the verification.
    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    /// Returns the size established by the verification.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns whether this proof exactly matches the recorded artifact
    /// integrity information.
    pub fn matches(&self, expected: &ContentDigest, expected_size: u64) -> bool {
        self.digest == *expected && self.size == expected_size
    }
}

/// Failure during integrity verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityError {
    expected: ContentDigest,
    actual: ContentDigest,
}

impl IntegrityError {
    /// Creates an integrity verification error.
    pub fn new(expected: ContentDigest, actual: ContentDigest) -> Self {
        Self { expected, actual }
    }

    /// Returns the expected content digest.
    pub fn expected(&self) -> &ContentDigest {
        &self.expected
    }

    /// Returns the digest calculated from the retrieved content.
    pub fn actual(&self) -> &ContentDigest {
        &self.actual
    }
}

/// Verifies that retrieved content matches the recorded integrity information.
///
/// The supplied content is hashed with SHA-256 and compared with the
/// expected digest. A mismatch produces an integrity failure.
pub fn verify_integrity(content: &[u8], expected: &ContentDigest) -> Result<(), IntegrityError> {
    let actual = ContentDigest::new(content);

    if actual == *expected {
        Ok(())
    } else {
        Err(IntegrityError::new(expected.clone(), actual))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_proof_requires_matching_content() {
        let content = b"trusted content";
        let expected = ContentDigest::new(content);

        let proof = IntegrityProof::verify(content, &expected).unwrap();

        assert!(proof.matches(&expected, content.len() as u64));
        assert_eq!(proof.digest(), &expected);
        assert_eq!(proof.size(), content.len() as u64);
    }

    #[test]
    fn integrity_proof_rejects_mismatched_content() {
        let expected = ContentDigest::new(b"trusted content");

        assert!(IntegrityProof::verify(b"tampered content", &expected).is_err());
    }

    #[test]
    fn digest_uses_sha256() {
        let digest = ContentDigest::new(b"abc");

        assert_eq!(
            digest.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223\
             b00361a396177a9cb410ff61f20015ad"
                .replace(char::is_whitespace, "")
        );
        assert_eq!(digest.as_bytes().len(), SHA256_DIGEST_LENGTH);
        assert_eq!(ContentDigest::algorithm(), "sha-256");
    }

    #[test]
    fn digest_has_fixed_length() {
        let digest = ContentDigest::new(b"some content");

        assert_eq!(digest.as_bytes().len(), 32);
    }

    #[test]
    fn digest_is_deterministic() {
        let first = ContentDigest::new(b"same content");
        let second = ContentDigest::new(b"same content");

        assert_eq!(first, second);
    }

    #[test]
    fn different_content_produces_different_digest() {
        let first = ContentDigest::new(b"content-a");
        let second = ContentDigest::new(b"content-b");

        assert_ne!(first, second);
    }

    #[test]
    fn empty_content_has_real_sha256_digest() {
        let digest = ContentDigest::new(b"");

        assert_eq!(
            digest.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb924\
             27ae41e4649b934ca495991b7852b855"
                .replace(char::is_whitespace, "")
        );
    }

    #[test]
    fn from_bytes_preserves_digest() {
        let bytes = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];

        let digest = ContentDigest::from_bytes(bytes);

        assert_eq!(
            digest.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223\
             b00361a396177a9cb410ff61f20015ad"
                .replace(char::is_whitespace, "")
        );
    }

    #[test]
    fn matching_content_passes_integrity_verification() {
        let content = b"exact content";
        let digest = ContentDigest::new(content);

        assert!(verify_integrity(content, &digest).is_ok());
    }

    #[test]
    fn mismatching_content_fails_integrity_verification() {
        let content = b"actual content";
        let expected = ContentDigest::new(b"different content");

        let result = verify_integrity(content, &expected);

        assert!(result.is_err());

        let error = result.unwrap_err();

        assert_eq!(error.expected(), &expected);
        assert_eq!(error.actual(), &ContentDigest::new(content));
    }

    #[test]
    fn integrity_error_preserves_expected_and_actual_digests() {
        let expected = ContentDigest::new(b"expected");
        let actual = ContentDigest::new(b"actual");

        let error = IntegrityError::new(expected.clone(), actual.clone());

        assert_eq!(error.expected(), &expected);
        assert_eq!(error.actual(), &actual);
    }
}
