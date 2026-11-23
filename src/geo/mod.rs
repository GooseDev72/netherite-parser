//! Geography as this builder needs it: a code, a flag and a readable name.
//!
//! No local GeoIP database and no second lookup: the mihomo probe already asks
//! the exit who it is, and that answer is the only one consistent with what was
//! measured.

pub mod countries;
pub mod flags;
pub mod naming;

pub use naming::{country_name, published_name};
