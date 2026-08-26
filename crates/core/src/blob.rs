use std::fmt;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// Content-addressed identifier for a [`Blob`], computed as SHA-256 over the
/// content bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobId([u8; 32]);

impl BlobId {
    /// Constructs an ID from raw 32 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 32 bytes of the ID.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Renders the ID as a lowercase 64-character hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in &self.0 {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobId({self})")
    }
}

impl Serialize for BlobId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BlobIdVisitor;

        impl Visitor<'_> for BlobIdVisitor {
            type Value = BlobId;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a 64-character hexadecimal string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() != 64 {
                    return Err(E::custom("blob id must be 64 hex characters"));
                }
                let mut bytes = [0u8; 32];
                for (i, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let pair = std::str::from_utf8(pair).map_err(E::custom)?;
                    bytes[i] = u8::from_str_radix(pair, 16).map_err(E::custom)?;
                }
                Ok(BlobId(bytes))
            }
        }

        deserializer.deserialize_str(BlobIdVisitor)
    }
}

/// Immutable file content with a content-addressed hash.
///
/// Content is shared via [`Arc`] so blobs clone cheaply across snapshots,
/// diffs, and transforms.
#[derive(Clone)]
pub struct Blob {
    hash: BlobId,
    content: Arc<[u8]>,
}

impl Blob {
    /// Creates a blob, computing the SHA-256 content hash.
    #[must_use]
    pub fn new(content: impl Into<Arc<[u8]>>) -> Self {
        let content = content.into();
        let digest = Sha256::digest(&content);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);
        Self {
            hash: BlobId(hash),
            content,
        }
    }

    /// Creates a blob from owned bytes.
    #[must_use]
    pub fn from_bytes(content: impl Into<Vec<u8>>) -> Self {
        Self::new(Arc::<[u8]>::from(content.into()))
    }

    /// Returns the content-addressed hash.
    #[must_use]
    pub const fn hash(&self) -> BlobId {
        self.hash
    }

    /// Returns the content bytes.
    #[must_use]
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    /// Returns the number of content bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Returns `true` if the blob has no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

impl PartialEq for Blob {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for Blob {}

impl fmt::Debug for Blob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Blob")
            .field("hash", &self.hash)
            .field("len", &self.len())
            .field("content", &self.content)
            .finish()
    }
}

impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("Blob", 2)?;
        state.serialize_field("hash", &self.hash)?;
        state.serialize_field("content", &STANDARD.encode(self.content()))?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Repr {
            hash: BlobId,
            content: String,
        }

        let repr = Repr::deserialize(deserializer)?;
        let bytes = STANDARD.decode(repr.content).map_err(de::Error::custom)?;
        let blob = Blob::new(bytes);
        if blob.hash != repr.hash {
            return Err(de::Error::custom("blob hash does not match content"));
        }
        Ok(blob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_content_sensitive() {
        let a = Blob::from_bytes(b"hello");
        let b = Blob::from_bytes(b"hello");
        let c = Blob::from_bytes(b"hello!");
        assert_eq!(a.hash(), b.hash());
        assert_ne!(a.hash(), c.hash());
        assert_eq!(a.hash().to_hex().len(), 64);
    }

    #[test]
    fn json_round_trip() {
        let blob = Blob::from_bytes(b"\x00binary\xffcontent");
        let json = serde_json::to_string(&blob).unwrap();
        let back: Blob = serde_json::from_str(&json).unwrap();
        assert_eq!(back, blob);
        assert_eq!(back.content(), blob.content());
    }

    #[test]
    fn json_rejects_tampered_hash() {
        let blob = Blob::from_bytes(b"payload");
        let json = serde_json::to_string(&blob).unwrap();
        let tampered = json.replace(&blob.hash().to_hex(), &"00".repeat(32));
        let result: Result<Blob, _> = serde_json::from_str(&tampered);
        assert!(result.is_err());
    }
}
