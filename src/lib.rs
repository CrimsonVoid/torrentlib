//! TODO - Library documentation

#![crate_name = "libbittorrent"]
#![crate_type = "lib"]

#![cfg_attr(feature="bench", feature(test))]

#[macro_use]
mod macros;

mod util;

pub mod error;
pub mod bencode;
pub mod files;
pub mod torrent;
