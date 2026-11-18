//! WebDAV sources: mokuro-bunko or any WebDAV server holding volumes, read on
//! demand.
//!
//! * `client`: the HTTP side (listings, ranges, downloads), with the
//!   credentials, which never leave Rust.
//! * `propfind`: parsing a `PROPFIND` answer.
//! * `layout`: what a listing means as series and volumes.
//! * `sync`: listing a source into the library.
//! * `cache`: the LRU cache of what was fetched.
//! * `ranged`: a remote archive as `Read + Seek`, block by block.
//! * `remote`: where a remote page, text or cover comes from, and downloads.
//! * `progress`: progress shared with Mokuro Reader's `volume-data.json`.

pub mod cache;
pub mod client;
pub mod layout;
pub mod progress;
pub mod propfind;
pub mod ranged;
pub mod remote;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_server;
