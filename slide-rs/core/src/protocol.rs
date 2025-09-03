// Re-export the standalone `protocol` crate so existing `crate::protocol::*`
// imports continue to work within the `core` crate and downstream crates.
pub use protocol::*;