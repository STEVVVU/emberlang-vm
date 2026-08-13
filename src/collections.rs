//! Hash containers for compiler-internal tables.
//!
//! These alias `rustc_hash`'s `FxHashMap`/`FxHashSet` — the same hasher rustc
//! uses for its own interning and symbol tables. `FxHash` is a few multiply-xor
//! rounds with no random seed, so it is markedly faster than SipHash-1-3 on the
//! short keys a compiler actually hashes: identifiers, register numbers, and
//! label ids.
//!
//! It is **not** collision-resistant and must never be used on attacker-chosen
//! keys. Every use here is on data the compiler itself produced from source it
//! has already accepted, and the VM's function-name table is built by codegen
//! rather than read from an untrusted file, so a hash-flooding input has no way
//! in. The resource limits in `crate::vm` bound execution regardless.
//!
//! One consequence to keep in mind: like `std::collections::HashMap`, iteration
//! order is unspecified. Anything user-visible must be sorted before it is
//! rendered — see `resolve::unknown_name`, which sorts candidates before
//! choosing a spelling suggestion.

pub type HashMap<K, V> = rustc_hash::FxHashMap<K, V>;
pub type HashSet<T> = rustc_hash::FxHashSet<T>;
