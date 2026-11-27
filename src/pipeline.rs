//! pipeline - entry point. Implementations in `pipeline/`.

pub mod combine;
pub mod run;

pub use run::{run, RunReport};
