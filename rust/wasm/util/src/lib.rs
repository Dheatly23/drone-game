#![allow(clippy::deref_addrof, clippy::missing_safety_doc)]

#[cfg(feature = "buffer")]
mod buffer;
mod log;

#[cfg(feature = "buffer")]
pub use buffer::*;
pub use log::*;
