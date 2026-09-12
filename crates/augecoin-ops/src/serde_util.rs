//! Serde helpers for values that must survive JSON without precision loss.
//!
//! Monetary amounts are 64-bit augesat that can exceed JavaScript's safe
//! integer range (2^53), so they are serialized as decimal strings. Clients
//! parse them with BigInt, preserving exactness for the financial dashboard.

pub mod i64_str {
    use serde::Serializer;

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
}
