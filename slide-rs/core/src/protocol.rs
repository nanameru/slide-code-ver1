// Re-export the standalone `protocol` crate so existing `crate::protocol::*`
// imports continue to work within the `core` crate and downstream crates.
pub use protocol::*;
// Keep a private alias to avoid unused import warnings in some editors.
use protocol as _protocol_reexport_anchor;