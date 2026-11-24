//! Netherite: subscription parser/builder CLI.
//!
//! Architecture:
//! - `netherite-core` (`core/`): reusable evidence-based validation engine.
//! - `src/`: thin CLI/adapters over core + xray probe runtime + output formatting.
//!
//! The core is the source of truth for validation policy; src adapters feed
//! probe evidence from real runtime tunnels (Xray/v2keylib).

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

pub use netherite_core as core;
