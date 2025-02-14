#![allow(clippy::deref_addrof, clippy::missing_safety_doc)]

#[cfg(feature = "buffer")]
mod buffer;
#[cfg(all(feature = "getrandom", getrandom_backend = "custom"))]
mod getrandom;
mod log;

#[cfg(feature = "buffer")]
pub use buffer::*;
pub use log::*;
