//! Secret wrappers: ZeroizeOnDrop, Debug = [REDACTED], no Display.

use zeroize::{Zeroize, ZeroizeOnDrop};

/// Opaque UTF-8 secret string. Never implement Display / Serialize.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn into_inner(mut self) -> String {
        // Take ownership without leaving a clone behind; ZeroizeOnDrop still runs on empty.
        std::mem::take(&mut self.0)
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}

/// Opaque material bytes (typically UTF-8 JSON). Zeroized on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MaterialBytes(Vec<u8>);

impl MaterialBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn from_utf8_str(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }
}

impl std::fmt::Debug for MaterialBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MaterialBytes([REDACTED])")
    }
}

impl serde::Serialize for MaterialBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Store as base64-free UTF-8 string when valid; else raw bytes as array.
        match std::str::from_utf8(&self.0) {
            Ok(s) => serializer.serialize_str(s),
            Err(_) => serializer.serialize_bytes(&self.0),
        }
    }
}

impl<'de> serde::Deserialize<'de> for MaterialBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = MaterialBytes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("string or bytes for MaterialBytes")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(MaterialBytes::from_utf8_str(v))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(MaterialBytes::new(v.into_bytes()))
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(MaterialBytes::new(v.to_vec()))
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(MaterialBytes::new(v))
            }
        }
        deserializer.deserialize_any(V)
    }
}
