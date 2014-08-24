//! TODO - module documentation

extern crate time;
extern crate serialize;

use self::time::Timespec;
use self::serialize::hex::ToHex;
use std::io::extensions::Bytes;
use std::io::fs;

use bencode;
use bencode::{BString, BInt, BList, BDict};

/// Single File
#[deriving(Show, PartialEq)]
pub struct File {
    /// Filename
    pub name: String,
    /// Size of file in bytes
    pub length: i64,
    /// Optional md5sum of the file
    pub md5sum: Option<String>,
}

/// Multi-file structure
#[deriving(Show, PartialEq)]
pub struct Directory {
    /// Root directory where all `files` are under
    pub name: String,
    /// Vector of `File`'s; `File.name` is the full path under `Directory.name`
    pub files: Vec<File>,
}

#[deriving(Show, PartialEq)]
// TODO - Combine File and Directory?
pub enum FileMode {
    File      (File),
    Directory (Directory),
}

/// Tracker(s) to announce to
#[deriving(Show, PartialEq)]
pub enum Announce {
    /// Unique tracker
    Tracker (String),
    /// List of trackers if present
    List    (Vec<Vec<String>>),
}

/// Torrent info
#[deriving(Show, PartialEq)]
pub struct Torrent {
    /// Tracker to announce to; if 'announce-list' is present this is a `Vec<Vec<String>>`
    pub announce: Announce,

    /// Date the file was created
    pub creation_date: Option<time::Tm>,
    /// Torrent comment
    pub comment: Option<String>,
    /// Program used to created the torrent
    pub created_by: Option<String>,
    /// Encoding format used to generate the torrent
    pub encoding: Option<String>,

    // Info dictionary
    /// Indicates if it is a private torrent
    pub private: bool,
    /// Bytes in each piece
    pub piece_length: i64,
    /// Vector of SHA1 hashes for each `piece_length`
    pub pieces: Vec<String>,
    /// Single file or directory
    pub mode: FileMode,
}

macro_rules! unwrap(
    ($key:expr, $val:ident, $err:expr) => (
        match $key {
            Some($val(n)) => n,
            _             => return $err,
        }
    );

    ($key:expr, $val:ident) => (
        match $key {
            Some($val(n)) => Some(n),
            _             => None,
        }
    );
)

macro_rules! unwrap_bstring(
    ($key:expr, $err:expr) => (
        match $key {
            Some(BString(v)) => match String::from_utf8(v) {
                Ok(s)  => s,
                Err(_) => return $err,
            },
            _ => return $err,
        }
    );

    ($key:expr) => (
        match $key {
            Some(BString(v)) => String::from_utf8(v).ok(),
            _                => None,
        }
    );
)

macro_rules! conv(
    ($key:expr, $val:ident, $err:expr) => (
        match $key {
            $val(n) => n,
            _       => return $err,
        }
    );
)

impl Torrent {
    /// Consumes `bytes` to build a `Torrent` struct
    pub fn new<R: Reader>(brd: &mut Bytes<R>) -> Result<Torrent, &'static str> {
        let err      = Err("Torrent file incorrect");
        let mut benc = match bencode::Benc::build_tree(brd) {
            Ok(mut v) => unwrap!(v.swap_remove(0), BDict, err),
            Err(e)    => return Err(e),
        };
        let mut info = unwrap!(benc.pop_equiv(&b"info"), BDict, err);

        let mut t = Torrent {
            announce:      Tracker(String::new()),

            comment:       unwrap_bstring!(benc.pop_equiv(&b"comment")),
            created_by:    unwrap_bstring!(benc.pop_equiv(&b"created by")),
            encoding:      unwrap_bstring!(benc.pop_equiv(&b"encoding")),
            creation_date: Some(time::at_utc(
                    Timespec::new(unwrap!(benc.pop_equiv(&b"creation date"), BInt, err), 0)
                )),
            private:      match unwrap!(info.pop_equiv(&b"private"), BInt) {
                              Some(1) => true,
                              _       => false,
                          },
            piece_length: unwrap!(info.pop_equiv(&b"piece length"), BInt, err),
            pieces:       Vec::new(),
            mode:         Directory(Directory {
                name:  String::new(),
                files: Vec::new(),
            }),
        };

        // Prefer 'announce-list' over 'announce'
        t.announce = if benc.contains_key_equiv(&b"announce-list") {
            let lists = unwrap!(benc.pop_equiv(&b"announce-list"), BList, err);

            if lists.len() == 0 {
                // Fallback to 'announce' if 'announce-list' is invalid
                Tracker(unwrap_bstring!(benc.pop_equiv(&b"announce"), err))
            } else {
                let mut alist = Vec::with_capacity(lists.len());

                for list in lists.move_iter() {
                    let list     = conv!(list, BList, err);
                    let mut flat = Vec::with_capacity(list.len());

                    for l in list.move_iter() {
                        match String::from_utf8(conv!(l, BString, err)) {
                            Ok(s)  => flat.push(s),
                            Err(_) => return err,
                        }
                    }

                    alist.push(flat);
                }

                List(alist)
            }
        } else { 
            Tracker(unwrap_bstring!(benc.pop_equiv(&b"announce"), err))
        };

        // Fill t.pieces with SHA1 hashes of each piece
        t.pieces = {
            let data      = unwrap!(info.pop_equiv(&b"pieces"), BString, err);
            let data      = data.as_slice();
            let split_len = 20;

            let splits = if data.len() % 20 != 0 {
                return err;
            } else {
                data.len() / split_len
            };
            let mut pieces = Vec::with_capacity(splits);

            for i in range(0, splits) {
                pieces.push(data.slice(i*split_len, (i+1)*split_len).to_hex());
            }

            pieces
        };

        t.mode = if info.contains_key_equiv(&b"files") {
            // Multi file
            let files = unwrap!(info.pop_equiv(&b"files"), BList, err);
            let mut d = Directory {
                name:  unwrap_bstring!(info.pop_equiv(&b"name"), err),
                files: Vec::with_capacity(files.len()),
            };

            // Populate `d.files`
            for file in files.move_iter() {
                let mut file = conv!(file, BDict, err);
                let mut f = File {
                    name:   String::new(),
                    length: unwrap!(file.pop_equiv(&b"length"), BInt, err),
                    md5sum: unwrap_bstring!(file.pop_equiv(&b"md5sum")),
                };

                let file_paths = unwrap!(file.pop_equiv(&b"path"), BList, err);
                for p in file_paths.move_iter() {
                    match String::from_utf8(conv!(p, BString, err)) {
                        Ok(s) => {
                            f.name.push_str(s.as_slice());
                            f.name.push_char('/');
                        },
                        Err(_) => return err,
                    }
                }
                // Remove trailing '/'
                f.name.pop_char();

                d.files.push(f);
            }

            Directory(d)
        } else {
            // Single file
            File(File {
                name:   unwrap_bstring!(info.pop_equiv(&b"name"), err),
                length: unwrap!(info.pop_equiv(&b"length"), BInt, err),
                md5sum: unwrap_bstring!(info.pop_equiv(&b"md5sum")),
            })
        };

        Ok(t)
    }

    /// Read from a file and make a `Torrent`
    pub fn from_file(file: &str) -> Result<Torrent, &'static str> {
        match fs::File::open(&Path::new(file)) {
            Ok(mut f) => Torrent::new(&mut f.bytes()),
            Err(e)    => Err(e.desc),
        }
    }
}

