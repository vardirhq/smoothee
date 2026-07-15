//! The deterministic Git command layer.
//!
//! Everything here shells out to the installed `git` binary and parses its
//! machine-readable output. This layer is the only place that mutates or
//! inspects the repository; the AI layer never reaches into it.

pub mod branches;
pub mod command;
pub mod repository;
pub mod status;

pub use repository::Repository;
