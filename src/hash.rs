//! Type aliases for AHash-backed collections used in hot paths.
//!
//! AHash is a non-cryptographic hasher that is 2-3x faster than the default
//! SipHash for internal data where DoS resistance is not needed.

/// A `HashMap` using `ahash::RandomState` as the hasher.
pub type AHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

/// A `HashSet` using `ahash::RandomState` as the hasher.
pub type AHashSet<T> = std::collections::HashSet<T, ahash::RandomState>;
