//! Authentication wire types.

use serde::{Deserialize, Serialize};

/// Opaque bearer token used by the daemon to authorize a client.
///
/// The wrapper exists so that callers can pass tokens around without
/// accidentally logging the raw string — never `Debug`-print it; use
/// [`AuthToken::redacted`] instead.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthToken(pub String);

impl AuthToken {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns a placeholder safe to log.
    pub fn redacted(&self) -> &'static str {
        "<redacted>"
    }
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AuthToken").field(&"<redacted>").finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing auth token")]
    Missing,
    #[error("invalid auth token")]
    Invalid,
    #[error("permission denied: {0}")]
    PermissionDenied(&'static str),
}

/// Constant-time equality compare for two byte slices, used everywhere a
/// secret (pairing code, device-token hash) is compared against attacker-
/// controlled input.
///
/// Backed by the `subtle` crate's `ConstantTimeEq` implementation, which is
/// the well-vetted constant-time primitive in the Rust ecosystem (the same
/// crate the `ring` and `dalek` families pull in). We pin `subtle` rather
/// than re-rolling our own subtle byte loop because LLVM aggressively
/// short-circuits naive Rust comparisons — `subtle` uses `core::hint::black_box`
/// and volatile reads internally to prevent that.
///
/// Unequal lengths return `false` immediately, but `subtle` ensures the
/// equal-length case is timing-uniform.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn ct_eq_equal() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn ct_eq_unequal_same_length() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn ct_eq_unequal_length() {
        assert!(!constant_time_eq(b"hello", b"helloo"));
        assert!(!constant_time_eq(b"", b"x"));
    }
}
