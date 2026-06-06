//! VM resource limits.
//!
//! The constants live in the shared `squidvm-limits` crate so the compiler
//! (`squidc-core`) enforces the exact same caps it must produce SQBC for.
//! Re-exported here so existing `crate::limits::MAX_*` references keep working.
pub use squidvm_limits::*;
