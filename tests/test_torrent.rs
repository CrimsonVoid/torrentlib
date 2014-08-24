extern crate time;

use self::time::Timespec;
use std::io::MemReader;

use torrent::torrent::{Tracker, Torrent, File};

#[test]
fn new() {
    // Internal details - torrent::from_file() calls torrent::new()
    let mut data = MemReader::new(Vec::from_slice(include_bin!("mock.torrent")));
    let result = match Torrent::new(&mut data.bytes()) {
        Ok(r)  => r,
        Err(e) => fail!(e),
    };

    let expect = Torrent {
        announce:      Tracker("https://tracker.example.com".to_string()),
        creation_date: Some(time::at_utc(Timespec::new(1408730037, 0))),
        comment:       Some("Hello World in Rust".to_string()),
        created_by:    Some("Transmission/2.82 (14160)".to_string()),
        encoding:      Some("UTF-8".to_string()),
        private:       false,
        piece_length:  32768,
        pieces: vec!(
            "63e4f9ace474ae11ef849c7089bf7c4afcd01a49".to_string(),
        ),
        mode: File(File {
            name:   "hello.rs".to_string(),
            length: 44,
            md5sum: None,
        }),
    };

    assert!(result == expect, "{} == {}", result, expect);
}
