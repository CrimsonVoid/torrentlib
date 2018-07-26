//! TODO - Library documentation

#![feature(rust_2018_preview)]
#![warn(rust_2018_idioms)]
#![crate_name = "libbittorrent"]
#![crate_type = "lib"]
#![cfg_attr(feature = "bench", feature(test))]

#[macro_use]
mod macros;

mod util;

pub mod bencode;
pub mod error;
pub mod files;
pub mod torrent;
