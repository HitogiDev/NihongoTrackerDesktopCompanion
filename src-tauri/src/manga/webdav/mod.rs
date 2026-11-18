//! WebDAV sources: mokuro-bunko or any WebDAV server holding volumes, read on
//! demand.
//!
//! * `client`: the HTTP side (listings, ranges, downloads), with the
//!   credentials, which never leave Rust.
//! * `propfind`: parsing a `PROPFIND` answer.
//! * `layout`: what a listing means as series and volumes.

// The pieces are wired into the library in the next steps.
#![allow(dead_code)]

pub mod client;
pub mod layout;
pub mod propfind;
#[cfg(test)]
pub(crate) mod test_server;
