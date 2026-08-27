//! Operation journaling and the mutating-operation engines built on it.
//!
//! Every mutation Smoothee performs is journaled here so it can be reversed and
//! diagnosed. [`journal`] is the append-only record; higher-level operation
//! modules create restore points, journal their work, and recover from it.

pub mod journal;
pub mod recovery;
pub mod resolve;
pub mod sync;
pub mod undo;
