//! NetheriteParser.
//!
//! A subscription builder with two judges and one policy: a node is published
//! only when a real client reaches the internet through it at a usable speed.
//! Every candidate is fetched, deduplicated and checked against a blacklist,
//! then run through a real Xray process and a real mihomo process. `sub/mihomo`
//! is the mihomo set; `sub/default` is the Xray set. Both use the same 15 Mbit/s
//! floor.
//!
//! Two judges matter because the parsers and transport implementations differ:
//! a config can work in Xray and be rejected by mihomo, and the other way
//! around. `meta.json` reports both sets separately.
//!
//! The modules are laid out the way a run happens:
//!
//! * [`sources`] — where links come from and which source earned its place,
//! * [`blacklist`] — addresses that are never published,
//! * [`collect`] — merge, deduplicate, keep provenance,
//! * [`geo`] — measured country, flag and the published name,
//! * [`check`] — the mihomo and Xray probes, the speed floor and the verdict,
//! * [`output`] — the two subscription files, `meta.json` and the dead cache,
//! * [`pipeline`] — the order those run in,
//! * [`report`] — what the run says about itself.

pub mod blacklist;
pub mod check;
pub mod cli;
pub mod collect;
pub mod config;
pub mod dedup;
pub mod error;
pub mod geo;
pub mod models;
pub mod output;
pub mod pipeline;
pub mod progress;
pub mod report;
pub mod settings;
pub mod sources;
pub mod util;

pub use error::{Error, Result};
