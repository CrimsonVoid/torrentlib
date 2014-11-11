use std::fmt;
use std::io;
use std::io::fs;
#[cfg(target_os = "linux")] use std::io::fs::PathExtensions;
use std::os;

/// Like the `try!` macro from std but also takes a function to call if there is an error. Useful
/// for cleanup
macro_rules! try_cb(
    ($val:expr, $err_cb:expr) => (
        match $val {
            Ok(o)  => o,
            Err(e) => { $err_cb(); return Err(e); },
        }
    )
)

/// Single File
#[deriving(PartialEq)]
pub struct File {
    /// Filename as described in the torrent file
    name: String,
    /// Download location as an absolute path
    path: Path,
    /// Size of file in bytes
    length: i64,
    /// Optional md5sum of the file
    md5sum: Option<String>,
}

impl File {
    /// Return a reference to where File is stored on disk
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Move File to `p`. This will return an `Err` if `p` is not an absolute path
    pub fn rename(&mut self, p: Path) -> io::IoResult<()> {
        if !p.is_absolute() {
            return Err(io::IoError{
                kind:   io::InvalidInput,
                desc:   "Not absolute path",
                detail: None,
            });
        }

        match fs::rename(&self.path, &p) {
            Ok(())   => { self.path = p; Ok(()) },
            e@Err(_) => e,
        }
    }

    /// Sets the location to `p` without moving the file.
    ///
    /// # Failure
    ///
    /// Fails if `p` is not an absolute path
    unsafe fn set_path(&mut self, p: Path) {
        assert!(p.is_absolute());
        self.path = p;
    }
}

impl fmt::Show for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::FormatError> {
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
    /// Vector of `File`'s; `File.name` is an absolute under `Directory.name`
    files: Vec<File>,
}

impl Directory {
    // FIXME - Avoid copies if moving on the same drive
    // TODO - File doesn't exist on disk yet
    /// Copies the files then deletes the original. Returns `Ok()` if the copy succeeds, even if
    /// it fails to delete the oringal. If any copy fails it will try to delete all copied files,
    /// ignoring any errors. Does not check if directory exists or if it overwrites files
    pub fn rename(&mut self, dir: Path) -> io::IoResult<()> {
        if !dir.is_absolute() {
            return Err(io::IoError {
                kind:   io::InvalidInput,
                desc:   "Must be an absolute path",
                detail: None,
            });
        }

        if dir == self.path {
            return Ok(());
        }
        try!(fs::mkdir_recursive(&dir, io::USER_DIR));

        // Transactionally move `self.path` to `dir`. We can not move `self.path` to `dir` because
        // it may contain files not managed by `Self`

        // Helper function to delete all files in `files` and then try to delete the directory
        // `p`. Ignores all errors when trying to delete
        let rollback = |p: &Path, files: &Vec<Path>| {
            for f in files.iter() {
                let _ = fs::unlink(f);

                // Needed because `rmdir(p)` will fail if not empty so try to delete directories
                // while iterating. This will prevent us from deleting the directory if a file
                // exists in `p` not in `files`
                let _ = fs::rmdir(&Path::new(f.dirname()));
           }

            let _ = fs::rmdir(p);
        };
        // `Files` that were successfully copied
        let mut files_copied = Vec::new();
        // `Paths` to delete if copies fail
        let mut files_rollback = Vec::new();

        // Copy files under `self.path` to `dir`
        // Should not need to modify `f` in this loop but needs to be mutable to modify later
        for f in self.files.iter_mut() {
            let child = match f.path().path_relative_from(&self.path) {
                Some(f) => dir.join(f.into_vec()),
                None    => continue,
            };

            try_cb!(
                fs::mkdir_recursive(&Path::new(child.dirname()), io::USER_DIR),
                || rollback(&dir, &files_rollback)
            );
            try_cb!(fs::copy(f.path(), &child), || rollback(&dir, &files_rollback));

            files_copied.push(f);
            files_rollback.push(child);
        }

        // Delete the original files
        for f in files_copied.iter_mut() {
            // Copied from `rollback()`
            let _ = fs::unlink(f.path());
            let _ = fs::rmdir(&Path::new(f.path().dirname()));
            
            // `files_copied` must have files under `self.path`
            let child = f.path().path_relative_from(&self.path).unwrap();
            unsafe { f.set_path(dir.join(child.into_vec())); }
        }
        let _ = fs::rmdir(&self.path);
        self.path = dir;

        Ok(())
    }
}

impl fmt::Show for Directory {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::FormatError> {
        write!(f, "Directory {{ path: {}, files: {} }}",
            self.path.display(), self.files)
    }
}

/// Try to find a suitable default default download directory. The Path is not guaranteed to exist
#[cfg(target_os = "linux")]
fn download_dir() -> Option<Path> {
    // $XDG_DOWNLOAD_DIR, $HOME/{D,d}ownload(s)
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

    // Default to `home_dir + "Downloads"` if a download directory was not found
    home_dir.push("Downloads");

    Some(home_dir)
}

#[cfg(target_os = "macos")]
fn download_dir() -> Option<Path> {
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
fn download_dir() -> Option<Path> {
    // $HOME/Downloads
    // TODO - Use `SHGetKnownFolderPath`
    // http://msdn.microsoft.com/en-us/library/windows/desktop/bb762188(v=vs.85).aspx
    // https://github.com/retep998/winapi-rs - Waiting on `CoMemFree`

    match os::homedir() {
        Some(mut p) => if p.is_absolute() { p.push("Downloads"); Some(p) }
                       else { None },
        _           => None,
    }
}
