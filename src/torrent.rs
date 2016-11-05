/*
//! TODO - module documentation
extern crate time;
extern crate hyper;

use std::collections;
use std::fs;
use std::io::Read;

use error;
use files;
use bencode::{self, Benc};

// Enum to represent a `File` or `Directory`
pub enum FileOrDir {
    File(files::File),
    Directory(files::Directory),
}

// Tracker(s) to announce to
type AnnounceList = Vec<String>;

fn announce_list(dict: &mut collections::HashMap<Vec<u8>, Benc>) -> Option<Vec<AnnounceList>> {
    // Torrent must have "announce" even if "announce-list" is found. Don't abort if "announce"
    // is not found, try "announce-list".
    // RFC - This is not BEP 003 compliant
    let announce = match dict.remove(&b"announce"[..]) {
        Some(Benc::String(s)) => String::from_utf8(s).ok().map(|s| vec![ vec![s] ]),
        _                     => None,
    };

    // try "announce-list", fall back to `announce` if any errors while parsing "announce-list"
    let lists        = unwrap_opt!(Benc::List, dict.remove(&b"announce-list"[..]), announce);
    let mut trackers = Vec::with_capacity(lists.len());

    for list in lists {
        let list          = unwrap!(Benc::List, list, announce);
        let mut announcer = Vec::with_capacity(list.len());

        for l in list {
            let l = unwrap!(Benc::String, l, announce);
            announcer.push(unwrap!(Ok, String::from_utf8(l), announce));
        }
        trackers.push(announcer);
    }

    Some(trackers)
}

// UTF-8 encoded
// TODO - Inline `Info` to `Torrent?
struct Info {
    /// Number of bytes in each piece
    piece_length: u64,
    /// SHA1 hashes mapped to each `piece_length` piece
    pieces: Vec<u8>,
    private: bool,

    /// Is it a `File` or a `Directory`
    files: FileOrDir,
}

impl Info {
    fn from_dict(dict: &mut collections::HashMap<Vec<u8>, Benc>) -> Option<Info> {
        let pieces    = unwrap_opt!(Benc::String, dict.remove(&b"pieces"[..]));
        let piece_len = 20;
        if pieces.len() % piece_len != 0 {
            return None;
        }

        // "files" will only be present if torrent info is multi-file
        let files = match dict.contains_key(&b"files"[..]) {
            true  => FileOrDir::Directory(unwrap!(Some, files::Directory::from_dict(dict))),
            false => FileOrDir::File(unwrap!(Some, files::File::from_dict(dict))),
        };

        let piece_length = unwrap_opt!(Benc::Int, dict.remove(&b"piece length"[..]));
        if piece_length < 0 {
            return None
        }

        Some(Info {
            piece_length: piece_length as u64,
            pieces:       pieces,
            private:      dict.remove(&b"private"[..]) == Some(Benc::Int(1)),
            files:        files,
        })
    }
}

pub struct Torrent {
    /// URL(s) to announce to. If only "announce" is present this is essentially `[[Tracker]]`
    trackers: Vec<AnnounceList>,
    info: Info,

    /// Date the torrent file was created in UNIX epoch
    creation_date: Option<time::Tm>,
    /// Name and version of program used to create the torrent
    created_by: Option<String>,
    comment: Option<String>,
}

impl Torrent {
    /// Try to read and parse a torrent file from a URL or a local file
    pub fn new(path: &str) -> error::Result<Torrent> {
        if path.starts_with("http://") || path.starts_with("https://") {
            Torrent::new_url(path)
        } else if path.starts_with("magnet:?") {
            Torrent::new_magnet(path)
        } else {
            Torrent::new_file(path)
        }
    }

    /// Try to create a Torrent from a stream of Bytes
    fn read<R: Read>(r: &mut R) -> error::Result<Torrent> {
        match Benc::new(&mut r.bytes()) {
            Ok(mut n) =>
                if n.is_empty() {
                    Err(error::Error::Other("No bencode nodes"))
                } else {
                    Torrent::from_benc(n.swap_remove(0))
                },
            Err(e) => Err(e),
        }
    }

    /// Open and parse a local file to create a Torrent
    fn new_file(filename: &str) -> error::Result<Torrent> {
        let mut f = try!(fs::File::open(filename));

        Torrent::read(&mut f)
    }

    /// Open and parse a torrent file from a URL to create a Torrent
    fn new_url(url: &str) -> error::Result<Torrent> {
        // TODO - Consider using a global pool?
        let client = hyper::client::Client::new();
        let mut res = match client.get(url).send() {
            Ok(r)  => r,
            Err(e) => match e {
                // TODO - Lossless errors?
                hyper::error::Error::Io(e) => return Err(error::Error::from(e)),
                _ => return Err(error::Error::Other("Could not download torrent")),
            },
        };

        Torrent::read(&mut res)
    }

    /// Open and parse a magnet link to create a Torrent
    fn new_magnet(magnet: &str) -> error::Result<Torrent> {
        // TODO - Add magnet support
        unimplemented!()
    }

    /// Create a Torrent from Benc nodes
    fn from_benc(nodes: bencode::Benc) -> error::Result<Torrent> {
        let mut dict = match nodes {
            Benc::Dict(d) => d,
            _             => return Err(error::Error::Other("Dictionary not found")),
        };

        let trackers = match announce_list(&mut dict) {
            Some(t) => t,
            None    => return Err(error::Error::Other("Announcers not found")),
        };

        let info = match Info::from_dict(&mut dict) {
            Some(t) => t,
            None    => return Err(error::Error::Other("Info not found")),
        };

        let creation_date = match dict.remove(&b"creation_date"[..]) {
            Some(Benc::Int(t)) => Some(time::at_utc(time::Timespec::new(t, 0))),
            _                  => None,
        };

        let created_by = match dict.remove(&b"created_by"[..]) {
            Some(Benc::String(s)) => String::from_utf8(s).ok(),
            _                     => None,
        };

        let comment = match dict.remove(&b"comment"[..]) {
            Some(Benc::String(s)) => String::from_utf8(s).ok(),
            _                     => None,
        };

        Ok(Torrent {
            trackers: trackers,
            info:     info,

            creation_date: creation_date,
            created_by:    created_by,
            comment:       comment,
        })
    }
}

// TODO - torrent::builder
*/
