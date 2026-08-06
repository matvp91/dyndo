//! Writing a fragmented subtitle out as a CMAF track.
//!
//! One module per container, each taking the fragments
//! [`fragmenter`](crate::fragmenter) produced and deciding what its format makes
//! of them. [`wvtt`] is the only one so far.

mod atoms;
pub mod wvtt;
