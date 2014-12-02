use std::default;
use std::error;
use std::fmt;
use std::os;
use std::io;
use std::io::fs;
use std::mem;
use std::path::GenericPath;
#[cfg(target_os = "linux")] use std::io::fs::PathExtensions;

#[deriving(PartialEq)]
pub enum Status {
    NotCreated,
    Downloading,
    Stopped,
    Seeding,
    Skip,
    Done,
    /// Can contian the last known location of the file
    Missing(Option<Path>),
    /// An optional string describing the error
    Other(Option<String>),
}

impl default::Default for Status {
    fn default() -> Status { Status::NotCreated }
}

impl fmt::Show for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Status::NotCreated    => write!(f, "NotCreated"),
            Status::Downloading   => write!(f, "Downloading"),
            Status::Stopped       => write!(f, "Stopped"),
            Status::Seeding       => write!(f, "Seeding"),
            Status::Skip          => write!(f, "Skip"),
            Status::Done          => write!(f, "Done"),
            Status::Other(ref s)  => write!(f, "Other({})", s),
            Status::Missing(None) => write!(f, "Missing(None)"),
            Status::Missing(Some(ref s)) => write!(f, "Missing(Some({}))", s.display()),
        }
    }
}

#[deriving(Show)]
pub enum MvError<'a> {
    /// A generic IoError
    IoError(io::IoError),
    /// Errors while moving `File`'s. Tuple of a reference to the `File` and the `IoError` that
    /// occoured
    MoveErrors(Vec<(&'a File, io::IoError)>),
}

impl<'a> error::FromError<io::IoError> for MvError<'a> {
    fn from_error(err: io::IoError) -> MvError<'a> { MvError::IoError(err) }
}

impl<'a> error::FromError<Vec<(&'a File, io::IoError)>> for MvError<'a> {
    fn from_error(err: Vec<(&'a File, io::IoError)>) -> MvError<'a> { MvError::MoveErrors(err) }
}

/// Single File
#[deriving(PartialEq)]
pub struct File {
    /// Filename as described in the torrent file
    name: String,
    /// Download location as an absolute path. The file might not exist, see `status` for more
    /// information
    path: Path,
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
    /// # Failure
    ///
    /// `path` is not an absolute path
    pub fn new(name: String, path: Path, length: u64) -> File {
        assert!(path.is_absolute());

        File{
            name:   name,
            path:   path,
            length: length,
            md5sum: None,
            status: default::Default::default(),
        }
    }

    /// Return a reference to where File should be stored on disk
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Move `File` to an absolute path `p`. If the status is `NotCreated` or `Missing` the path
    /// is set without attempting to move the file.
    pub fn mv(&mut self, p: Path) -> io::IoResult<()> {
        if !p.is_absolute() {
            return Err(io::IoError {
                kind:   io::IoErrorKind::InvalidInput,
                desc:   "Not an absolute path",
                detail: None,
            });
        }

        match self.status {
            Status::NotCreated | Status::Missing(_) => { self.path = p; return Ok(()) },
            _ => (),
        }

        // Will succeed if folder exists
        try!(fs::mkdir_recursive(&Path::new(p.dirname()), io::USER_DIR));

        let old_path = mem::replace(&mut self.path, p);
        match fs::rename(&old_path, &self.path) {
            e@Ok(_)  => e,
            e@Err(_) => { self.status = Status::Missing(Some(old_path)); e },
        }
    }

    /// Sets the location to `p` without moving or checking if the file exists.
    ///
    /// # Failure
    ///
    /// Fails if `p` is not an absolute path
    pub unsafe fn set_path(&mut self, p: Path) {
        assert!(p.is_absolute());
        self.path = p;
    }
}

impl fmt::Show for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        // File { name: hello.rs, length: 44, md5sum: None }
        write!(f, "File {{ name: {}, path: {}, length: {}, md5sum: {} }}",
            self.name, self.path.display(), self.length, self.md5sum)
    }
}

/// Multi-file structure
#[deriving(PartialEq)]
pub struct Directory {
    /// Root directory where all `files` are under. This must be an absolute path.
    path: Path,
    /// Vector of `File`'s
    files: Vec<File>,
    /// Status of the Directory, independant from the files owned by Self
    pub status: Status,
}

impl Directory {
    /// Create a new `Directory` with `path`
    ///
    /// # Failure
    ///
    /// `path` is not an absolute path
    pub fn new(path: Path) -> Directory {
        Directory::with_capacity(path, 0)
    }

    /// Create a new `Directory with `path` and reserve space for `cap` Files
    ///
    /// # Failure
    ///
    /// `path` is not an absolute path
    pub fn with_capacity(path: Path, cap: uint) -> Directory {
        assert!(path.is_absolute());

        Directory {
            path:   path,
            files:  Vec::with_capacity(cap),
            status: Status::NotCreated,
        }
    }

    /// Add a `File` to be managed by the `Directory`. See `add_files` for more details.
    pub fn add_file(&mut self, file: File) {
        self.files.push(file)
    }

    /// Move `files` to be owned by the `Directory`. Location of Files will not be changed.
    pub fn add_files(&mut self, files: Vec<File>) {
        if self.files.len() == 0 {
            self.files = files;
            return;
        }

        self.files.extend(files.into_iter());
    }

    /// Move all files under `self.path` to `dir`. `dir` must be an absolute path. Errors while
    /// moving files are accumulated and returned as `MvError::MoveErrors`. Status of files in
    /// `MvError::MoveErrors` are independent from the error.
    pub fn mv(&mut self, dir: Path) -> Result<(), MvError> {
        if !dir.is_absolute() {
            return Err(MvError::IoError(io::IoError {
                kind:   io::IoErrorKind::InvalidInput,
                desc:   "Must be an absolute path",
                detail: None,
            }));
        }

        if dir == self.path { return Ok(()); }

        try!(fs::mkdir_recursive(&dir, io::USER_DIR));
        let mut errs = Vec::new();

        {
            let path_v = self.path.as_vec();

            for f in self.files.iter_mut().by_ref() {
                // The path to move the file to
                let new_path = {
                    let file_v = f.path.as_vec();

                    // Make sure `self.path` is fully an ancestor of `f.path` to avoid moving
                    // files that are not under `self.path`
                    if file_v.len() > path_v.len() && path_v == file_v[..path_v.len()] {
                        match f.path.path_relative_from(&self.path) {
                            Some(p) => dir.join(p),
                            None    => continue, // This should be unreachable
                        }
                    } else {
                        continue
                    }
                };

                if let Err(e) = f.mv(new_path) {
                    errs.push((&*f, e));
                }
            }
        }

        // Deletes the folder if possible, will fail if `self.path` is not empty. We should be
        // able to continue regardless of error
        // TODO - Should we report something if this fails?
        let _ = fs::rmdir(&self.path);
        self.path = dir;

        if errs.len() == 0 { Ok(()) } else { Err(MvError::MoveErrors(errs)) }
    }
}

impl fmt::Show for Directory {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Directory {{ path: {}, files: {} }}",
            self.path.display(), self.files)
    }
}

/// Try to find a suitable default default download directory. The Path is not guaranteed to
/// exist, but will be an absolute path.
#[cfg(target_os = "linux")]
pub fn download_dir() -> Option<Path> {
    // $XDG_DOWNLOAD_DIR, $HOME/{D,d}ownload{,s}
    match os::getenv("XDG_DOWNLOAD_DIR") {
        Some(d) => match Path::new_opt(d) {
            Some(p) => return Some(p),
            _       => (),
        },
        _ => (),
    }

    let mut home_dir = match os::homedir() {
        Some(p) => if p.is_absolute() { p } else { return None },
        None    => return None,
    };

    if home_dir.is_dir() {
        for d in ["Downloads", "downloads", "Download", "download"].iter() {
            let dir = home_dir.join(*d);

            if dir.is_dir() {
                return Some(dir)
            }
        }
    }

    // Default to `home_dir + "Downloads"`
    home_dir.push("Downloads");

    Some(home_dir)
}

#[cfg(target_os = "macos")]
pub fn download_dir() -> Option<Path> {
    // $HOME/Downloads
    // TODO - Use `NSSearchPathForDirectoriesInDomains`
    // https://developer.apple.com/library/ios/documentation/Cocoa/Reference/Foundation/Miscellaneous/Foundation_Functions/index.html#//apple_ref/doc/uid/20000055-181040

    match os::homedir() {
        Some(mut p) => if p.is_absolute() { p.push("Downloads"); Some(p) }
                       else { None },
        _           => None,
    }
}

#[cfg(target_os = "windows")]
pub fn download_dir() -> Option<Path> {
    // $HOME/Downloads
    // TODO - Use `SHGetKnownFolderPath`
    // http://msdn.microsoft.com/en-us/library/windows/desktop/bb762188
    // https://github.com/retep998/winapi-rs - Waiting on `CoTaskMemFree`

    match os::homedir() {
        Some(mut p) => if p.is_absolute() { p.push("Downloads"); Some(p) }
                       else { None },
        _           => None,
    }
}

#[cfg(test)]
mod test_file {
    use std::os;
    use super::{File, Status};

    fn name()     -> String { "こんにちは".into_string() }
    fn path_abs() -> Path   { os::tmpdir().join(name()) }
    static LEN: u64 = 256;

    #[test]
    fn new() {
        let name = name();
        let path = path_abs();
        let f = File::new(name.clone(), path.clone(), LEN);

        assert!(f.name   == name, "{} == {}", f.name, name);
        assert!(f.path   == path, "{} == {}", f.path.display(), path.display());
        assert!(f.length == LEN,  "{} == {}", f.length, LEN);
        assert!(f.md5sum == None, "{} == None", f.md5sum);
        assert!(f.status == Status::NotCreated, "{} == {}", f.status, Status::NotCreated);
    }
}

#[cfg(test)]
mod test_directory {
    use std::os;
    use super::{File, Directory, Status};

    fn name()     -> String { "こんにちは".into_string() }
    fn path_abs() -> Path   { os::tmpdir().join(name()) }
    static LEN: u64  = 256;
    static CAP: uint = 8;

    #[test]
    fn new() {
        let path = path_abs();
        let d = Directory::with_capacity(path.clone(), CAP);

        assert!(d.path             == path, "{} == {}", d.path.display(), path.display());
        assert!(d.files.capacity() == CAP,  "{} == {}", d.files.capacity(), CAP);
        assert!(d.files.len()      == 0,    "{} == 0",  d.files.len());
        assert!(d.status == Status::NotCreated, "{} == {}", d.status, Status::NotCreated);
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

        let mut dir   = Directory::new(path.clone());
        let mut files = Vec::with_capacity(CAP);
        let mut copy  = Vec::with_capacity(CAP);

        for i in range(0, CAP as u64) {
            let name  = format!("file-{}.ext", i);
            let fpath = path.join(name.clone());

            files.push(File::new(name.clone(), fpath.clone(), i*i));
            copy.push(File::new(name, fpath, i*i));
        }

        dir.add_files(files);
        assert!(dir.files == copy);
    }

    #[test]
    fn mv() {
        let tmp = os::tmpdir().join("root");
        // Default `path` for `dir`
        let path = tmp.join("default");
        // Path to place files not owned by `dir`
        let other = tmp.join("somewhere_else");
        // Path to move files to
        let moved = os::tmpdir().join("moved");

        let mut dir = Directory::new(path.clone());

        for i in range(0, CAP as u64) {
            let name  = format!("file-{}.ext", i);
            let fpath = if i % 2 == 0 { &path } else { &other }.join(name.clone());

            dir.add_file(File::new(name, fpath, i*i));
        }

        if let Err(e) = dir.mv(moved.clone()) {
            panic!("Failed to moved files. {}", e);
        }

        // Check if only files under `root` were moved to `moved`
        for (i, f) in dir.files.iter().enumerate() {
            let p = if i % 2 == 0 { &moved } else { &other };

            assert!(f.path().dirname() == p.as_vec());
            assert!(f.path().filename().unwrap() == format!("file-{}.ext", i).as_bytes());
        }
    }
}
