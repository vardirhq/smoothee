//! Operation journaling and (in later phases) undo/plan machinery.
//!
//! Every mutation Smoothee performs is journaled here so it can be reversed and
//! diagnosed. Phase 1 establishes the journal; `undo` and `plan` build on it.
//!
//! The journal API is fully implemented and unit-tested now so that Phase 2's
//! `sync` (the first mutating command) can record operations on day one. It is
//! deliberately not yet called from any command, hence the allow below.
#![allow(dead_code)]

pub mod journal;
