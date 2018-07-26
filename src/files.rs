use std::collections;
use std::convert;
use std::default;
use std::env;
use std::ffi;
use std::fs;
use std::io;
use std::mem;
use std::path;

use crate::bencode::Benc;
use crate::util;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotCreated,
    Downloading,
    Stopped,
    Seeding,
    Skip,
    Done,
    /// Can contian the last known location of the file
    Missing(Option<path::PathBuf>),
    /// An optional string describing the error
    Other(Option<String>),
}

impl default::Default for Status {
    fn default() -> Status {
        Status::NotCreated
    }
}

#[derive(Debug)]
pub enum MvError<'a> {
    /// A generic IoError
    Io(io::Error),
    /// Errors while moving `File`'s. Tuple of a reference to the `File` and the `IoError` that
    /// occoured
    MoveErrors(Vec<(&'a File, io::Error)>),
}

impl<'a> convert::From<io::Error> for MvError<'a> {
    fn from(e: io::Error) -> MvError<'a> {
        MvError::Io(e)
    }
}

impl<'a> convert::From<Vec<(&'a File, io::Error)>> for MvError<'a> {
    fn from(e: Vec<(&'a File, io::Error)>) -> MvError<'a> {
        MvError::MoveErrors(e)
    }
}

/// Single File
#[derive(Debug, PartialEq, Eq)]
pub struct File {
    /// Filename as described in the torrent file
    name: String,
    /// Download location as an absolute path. The file might not exist, see `status` for more
    /// information
    path: path::PathBuf,
    /// Size of file in bytes
    length: u64,
    /// Optional md5sum of the file
    pub md5sum: Option<String>,
    /// Status of File
    pub status: Status,
}

impl File {
    /// Create a new `File` with:
    ///     * `name`   - Usually as described in the .torrent file
    ///     * `path`   - Location to create the file on disk
    ///     * `length` - Size of the file in bytes
    ///
    /// # Panics
    ///
    /// `path` is not an absolute path
    pub fn new(name: String, path: path::PathBuf, length: u64) -> File {
        assert!(path.is_absolute());

        File {
            name: name,
            path: path,
            length: length,
            md5sum: None,
            status: Status::NotCreated,
        }
    }

    /// Create a new `File` from a HashMap; the hashmap must contain "name" and "length" keys with
    /// an optional "md5sum" key
    pub fn from_dict(dict: &mut collections::HashMap<Vec<u8>, Benc>) -> Option<File> {
        let md5sum = match dict.remove(&b"md5sum"[..]) {
            // TODO - Check if it is a valid hash
            Some(Benc::String(s)) => String::from_utf8(s).ok(),
            _ => None,
        };

        // name_raw should be a Vec<String>, where each element is a subfolder
        let name_raw = unwrap_opt!(Benc::List, dict.remove(&b"name"[..]));
        let mut name = String::new();
        let mut path = util::download_dir().unwrap_or_else(env::temp_dir);

        for part in name_raw {
            let part = unwrap!(Benc::String, part);
            let part_str = unwrap!(Ok, ::std::str::from_utf8(&part));

            name.push_str(part_str);

            if part_str != ".." || part_str != "." {
                path.push(part_str);
            }
        }

        let length = unwrap_opt!(Benc::Int, dict.remove(&b"length"[..]));

        Some(File {
            name: name,
            path: path,
            length: if length < 0 {
                return None;
            } else {
                length as u64
            },
            md5sum: md5sum,
            status: Status::NotCreated,
        })
    }

    /// Return a reference to where File should be stored on disk
    pub fn path(&self) -> &path::Path {
        &self.path
    }

    /// Move `File` to an absolute path `p`. If the status is `NotCreated` or `Missing` the path
    /// is set without attempting to move the file.
    pub fn set_location(&mut self, mut p: path::PathBuf) -> io::Result<()> {
        if !p.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not an absolute path",
            ));
        }

        match self.status {
            Status::NotCreated | Status::Missing(_) => {
                self.path = p;
                return Ok(());
            }
            _ => (),
        }

        // will succeed if folder exists
        // TODO - This will fail if we try to move to /
        match p.parent() {
            Some(p) => try!(fs::create_dir_all(p)),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "No parent folder",
                ))
            }
        }

        mem::swap(&mut self.path, &mut p);
        // TODO - This will not work if the new name is on a different mount point.
        match fs::rename(&p, &self.path) {
            e @ Ok(_) => e,
            e @ Err(_) => {
                self.status = Status::Missing(Some(p));
                e
            }
        }
    }
}

/// Multi-file structure
#[derive(Debug, PartialEq, Eq)]
pub struct Directory {
    /// Root directory where all `files` are under. This must be an absolute path.
    path: path::PathBuf,
    /// Vector of `File`'s
    files: Vec<File>,
    /// Status of the Directory, independant from the files owned by Self
    pub status: Status,
}

impl Directory {
    /// Create a new `Directory` with `path`
    ///
    /// # Panics
    ///
    /// `path` is not an absolute path
    pub fn new(path: path::PathBuf) -> Directory {
        Directory::with_capacity(path, 0)
    }

    /// Create a new `Directory` with `path` and reserve space for `cap` Files
    ///
    /// # Panics
    ///
    /// `path` is not an absolute path
    pub fn with_capacity(path: path::PathBuf, cap: usize) -> Directory {
        assert!(path.is_absolute());

        Directory {
            path: path,
            files: Vec::with_capacity(cap),
            status: Status::NotCreated,
        }
    }

    /// Create a new `Directory` from a HashMap. The HashMap must contain a "name" key and "files"
    /// list which should match `Files::from_dict()` requirements
    pub fn from_dict(dict: &mut collections::HashMap<Vec<u8>, Benc>) -> Option<Directory> {
        let mut path = util::download_dir().unwrap_or_else(env::temp_dir);
        for p in unwrap_opt!(Benc::String, dict.remove(&b"name"[..]))
            .split(|&c| c == b'/')
            .filter(|&p| p == b".." || p == b".")
        {
            if let Ok(s) = ::std::str::from_utf8(&util::sanitize_path(p)) {
                path.push(s);
            }
        }

        let fs = unwrap_opt!(Benc::List, dict.remove(&b"files"[..]));
        let mut files = Vec::with_capacity(fs.len());

        for f in fs {
            let mut f = unwrap!(Benc::Dict, f);
            files.push(unwrap!(Some, File::from_dict(&mut f)));
        }

        Some(Directory {
            path: path,
            status: Status::NotCreated,
            files: files,
        })
    }

    /// Add a `File` to be managed by the `Directory`. See `add_files` for more details.
    pub fn add_file(&mut self, file: File) {
        self.files.push(file)
    }

    /// Move `files` to be owned by the `Directory`. Location of Files will not be changed.
    pub fn add_files(&mut self, files: Vec<File>) {
        self.files.extend(files.into_iter());
    }

    /// Renames root folder
    /// From: /path/to/original/file.ext
    /// To:   /path/to/changed/file.ext
    pub fn rename<P>(&mut self, p: P) -> Result<(), MvError<'_>>
    where
        P: convert::AsRef<ffi::OsStr>,
    {
        let dir = (&self.path).with_file_name(p);
        self.set_location(dir)
    }

    /// Move all files under `self.path` to `dir`. `dir` must be an absolute path. Errors while
    /// moving files are accumulated and returned as `MvError::MoveErrors`. Status of files in
    /// `MvError::MoveErrors` are independent from the error.
    pub fn set_location(&mut self, dir: path::PathBuf) -> Result<(), MvError<'_>> {
        if !dir.is_absolute() {
            return Err(MvError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Must be an absolute path",
            )));
        }

        if dir == self.path {
            return Ok(());
        }

        try!(fs::create_dir_all(&dir));
        let mut errs = Vec::new();

        let path_len = match self.path.to_str() {
            Some(p) => p.len() + 1,
            None => {
                return Err(MvError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    "`self.path` is not a valid string",
                )))
            }
        };

        // move files under self.path
        for f in &mut self.files {
            if !f.path.starts_with(&self.path) {
                continue;
            }

            let new_path = match f.path.to_str() {
                Some(f) => dir.join(&f[path_len..]),
                None => continue,
            };

            if let Err(e) = f.set_location(new_path) {
                errs.push((&*f, e));
            }
        }

        // Deletes the folder if possible, will fail if `self.path` is not empty. We should be
        // able to continue regardless of error
        // TODO - Should we report something if this fails?
        let _ = fs::remove_dir(&self.path);
        self.path = dir;

        if errs.is_empty() {
            Ok(())
        } else {
            Err(MvError::MoveErrors(errs))
        }
    }
}

#[cfg(test)]
mod test_file {
    use std::borrow::ToOwned;
    use std::env;
    use std::path;

    use super::{File, Status};

    fn name() -> String {
        "こんにちは".to_owned()
    }
    fn path_abs() -> path::PathBuf {
        env::temp_dir().join(name())
    }
    fn path_rel() -> path::PathBuf {
        path::PathBuf::from(name())
    }
    static LEN: u64 = 256;

    #[test]
    fn new() {
        let name = name();
        let path = path_abs();
        let f = File::new(name.clone(), path.clone(), LEN);

        assert!(f.name == name, "{} == {}", f.name, name);
        assert!(f.path == path, "{:?} == {:?}", f.path, path);
        assert!(f.length == LEN, "{} == {}", f.length, LEN);
        assert!(f.md5sum == None, "{:?} == None", f.md5sum);
        assert!(
            f.status == Status::NotCreated,
            "{:?} == {:?}",
            f.status,
            Status::NotCreated
        );
    }

    #[test]
    #[should_panic]
    fn new_fail() {
        File::new(name(), path_rel(), LEN);
    }

    #[test]
    fn from_dict() {
        unimplemented!()
    }

    #[test]
    fn set_location() {
        let mut f = File::new(name(), path_abs(), LEN);
        let p = env::temp_dir().join("あ");

        if let Err(e) = f.set_location(p.clone()) {
            panic!("Could not move file. {}", e)
        }
        assert!(f.path() == p.as_path());

        if let Ok(_) = f.set_location(path::PathBuf::from("あ")) {
            panic!("Moved file to relative path")
        }
    }
}

#[cfg(test)]
mod test_directory {
    use std::borrow::ToOwned;
    use std::env;
    use std::ffi;
    use std::path;

    use super::{Directory, File, Status};

    fn name() -> String {
        "こんにちは".to_owned()
    }
    fn path_abs() -> path::PathBuf {
        env::temp_dir().join(name())
    }
    static LEN: u64 = 256;
    static CAP: usize = 8;

    #[test]
    fn new() {
        let path = path_abs();
        let d = Directory::with_capacity(path.clone(), CAP);

        assert!(d.path == path, "{:?} == {:?}", d.path, path);
        assert!(
            d.files.capacity() == CAP,
            "{} == {}",
            d.files.capacity(),
            CAP
        );
        assert!(d.files.len() == 0, "{} == 0", d.files.len());
        assert!(
            d.status == Status::NotCreated,
            "{:?} == {:?}",
            d.status,
            Status::NotCreated
        );
    }

    #[test]
    #[should_panic]
    fn new_fail() {
        Directory::new(path::PathBuf::from(""));
    }

    #[test]
    fn from_dict() {
        unimplemented!()
    }

    #[test]
    fn add_file() {
        let mut d = Directory::new(path_abs());

        let name = name();
        let path = path_abs().join("file");
        d.add_file(File::new(name.clone(), path.clone(), LEN));

        assert!(d.files.len() == 1);
        assert!(d.files[0] == File::new(name, path, LEN));
    }

    #[test]
    fn add_files() {
        let path = path_abs();

        let mut dir = Directory::new(path.clone());
        let mut files = Vec::with_capacity(CAP);
        let mut copy = Vec::with_capacity(CAP);

        for i in 0..CAP as u64 {
            let name = format!("file-{}.ext", i);
            let fpath = path.join(name.clone());

            files.push(File::new(name.clone(), fpath.clone(), i * i));
            copy.push(File::new(name, fpath, i * i));
        }

        dir.add_files(files);
        assert!(dir.files == copy);
    }

    #[test]
    fn rename() {
        let path = path_abs();
        let mut d = Directory::new(path.join("old"));

        if let Err(_) = d.rename("new") {
            panic!("Error while renaming directory");
        }

        assert!(d.path == path.join("new"));
    }

    #[test]
    fn set_location() {
        let tmp = env::temp_dir().join("root");
        // Default `path` for `dir`
        let path = tmp.join("default");
        // Path to place files not owned by `dir`
        let other = tmp.join("somewhere_else");
        // Path to move files to
        let moved = env::temp_dir().join("moved");

        let mut dir = Directory::new(path.clone());

        for i in 0..CAP as u64 {
            let name = format!("file-{}.ext", i);
            let fpath = if i % 2 == 0 { &path } else { &other }.join(name.clone());

            dir.add_file(File::new(name, fpath, i * i));
        }

        if let Err(e) = dir.set_location(moved.clone()) {
            panic!("Failed to moved files. {:?}", e);
        }

        // Check if only files under `root` were moved to `moved`
        for (i, f) in dir.files.iter().enumerate() {
            let p = if i % 2 == 0 { &moved } else { &other };

            assert!(f.path().parent() == Some(p));
            assert!(f.path().file_name().unwrap() == ffi::OsStr::new(&format!("file-{}.ext", i)));
        }
    }

    #[test]
    fn set_location_rel_path() {
        let mut d = Directory::new(path_abs());

        if let Ok(_) = d.set_location(path::PathBuf::from("")) {
            panic!("Moved directory to relative path");
        }
    }
}
