//! TODO - module documentation

extern crate time;

use std::fmt::{Show, Formatter, FormatError};
use std::io::{InvalidInput, IoError, IoResult, UserDir, fs};
use std::io::extensions::Bytes;
use std::io::fs::PathExtensions;
use std::os;

use self::time::Timespec;

use bencode;
use bencode::{BString, BInt, BList, BDict};


#[deriving(Show, PartialEq)]
pub enum FileMode {
    File      (FileM),
    Directory (DirectoryM),
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
    pub pieces: Vec<Vec<u8>>,
    /// Single file or directory
    pub mode: FileMode,
}

// TODO - Set directory absolute path before torrent is created
impl Torrent {
    /// Consumes `bytes` to build a `Torrent` struct
    pub fn new<R: Reader>(brd: &mut Bytes<R>) -> Result<Torrent, &'static str> {
        let err      = Err("Torrent file incorrect");
        let mut benc = match bencode::Benc::build_tree(brd) {
            Ok(mut v) => unwrap!(v.swap_remove(0), BDict, err),
            Err(e)    => return Err(e),
        };
        let mut info = unwrap!(benc.pop_equiv(&b"info"), BDict, err);

        // Prefer 'announce-list' over 'announce'
        let announce = if benc.contains_key_equiv(&b"announce-list") {
            // Vec<Vec<BString>> -> Vec<Vec<String>>
            let lists = unwrap!(benc.pop_equiv(&b"announce-list"), BList, err);

            if lists.len() == 0 {
                // Fallback to 'announce' if 'announce-list' is invalid
                Tracker(unwrap_bstring!(benc.pop_equiv(&b"announce"), err))
            } else {
                let mut alist = Vec::with_capacity(lists.len());

                for list in lists.into_iter() {
                    let list     = conv!(list, BList, err);
                    let mut flat = Vec::with_capacity(list.len());

                    for l in list.into_iter() {
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
        let pieces = {
            let data      = unwrap!(info.pop_equiv(&b"pieces"), BString, err);
            let split_len = 20;

            let splits = if data.len() % 20 == 0 {
                data.len() / split_len
            } else {
                return err;
            };
            let mut pieces = Vec::with_capacity(splits);

            for i in range(0, splits) {
                pieces.push(data[i*split_len..(i+1)*split_len].to_vec());
            }

            pieces
        };

        let mode = if info.contains_key_equiv(&b"files") {
            // Multi file
            let files = unwrap!(info.pop_equiv(&b"files"), BList, err);
            let mut d = DirectoryM {
                path:  conv!(
                    // TODO - Absolute path
                    Path::new_opt(unwrap_bstring!(info.pop_equiv(&b"name"), err)),
                    Some,
                    err
                ),
                files: Vec::with_capacity(files.len()),
            };

            // Populate `d.files`
            for file in files.into_iter() {
                let mut file = conv!(file, BDict, err);
                let mut f = FileM {
                    name:   String::new(),
                    path:   Path::new(""),
                    length: unwrap!(file.pop_equiv(&b"length"), BInt, err),
                    md5sum: unwrap_bstring!(file.pop_equiv(&b"md5sum")),
                };

                let file_paths = unwrap!(file.pop_equiv(&b"path"), BList, err);
                for p in file_paths.into_iter() {
                    f.path.push(conv!(p, BString, err));
                }

                f.name = match f.path.as_str() {
                    Some(s) => String::from_str(s),
                    None    => return err,
                };

                d.files.push(f);
            }

            Directory(d)
        } else {
            // Single file
            let path = unwrap_bstring!(info.pop_equiv(&b"name"), err);

            File(FileM {
                name:   path.clone(),
                path:   Path::new(path),
                length: unwrap!(info.pop_equiv(&b"length"), BInt, err),
                md5sum: unwrap_bstring!(info.pop_equiv(&b"md5sum")),
            })
        };

        Ok(Torrent {
            announce:      announce,
            comment:       unwrap_bstring!(benc.pop_equiv(&b"comment")),
            created_by:    unwrap_bstring!(benc.pop_equiv(&b"created by")),
            encoding:      unwrap_bstring!(benc.pop_equiv(&b"encoding")),
            creation_date: Some(time::at_utc(
                Timespec::new(unwrap!(benc.pop_equiv(&b"creation date"), BInt, err), 0)
            )),
            private:      unwrap!(info.pop_equiv(&b"private"), BInt) == Some(1),
            piece_length: unwrap!(info.pop_equiv(&b"piece length"), BInt, err),
            pieces:       pieces,
            mode:         mode,
        })
    }

    /// Read from a file and make a `Torrent`
    pub fn from_file(file: &str) -> Result<Torrent, &'static str> {
        match fs::File::open(&Path::new(file)) {
            Ok(mut f) => Torrent::new(&mut f.bytes()),
            Err(e)    => Err(e.desc),
        }
    }
}

