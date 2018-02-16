#[cfg(target_os = "windows")]
extern crate ole32;
#[cfg(target_os = "windows")]
extern crate shell32;
#[cfg(target_os = "windows")]
extern crate uuid;
#[cfg(target_os = "windows")]
extern crate winapi;

use std::borrow::Cow;
use std::env;
use std::path;

#[cfg(target_os = "linux")]
use std::fs;

#[cfg(target_os = "windows")]
use std::mem;
#[cfg(target_os = "windows")]
use std::ptr;
#[cfg(target_os = "windows")]
use std::slice;

// default download suffix
const DOWNLOAD_SUFFIX: &'static str = "Downloads";

#[cfg(target_os = "linux")]
fn valid_byte(b: u8) -> bool {
    b != 0
}

#[cfg(target_os = "macos")]
fn valid_byte(b: u8) -> bool {
    true
}

#[cfg(target_os = "windows")]
fn valid_byte(b: u8) -> bool {
    match b {
        b'\0' | b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|' => false,
        _ => true,
    }
}

/// Naively try to sanitize paths. This assumes you are writing to NTFS on Windows, HFS+ on OS X,
/// or Ext4/BTRFS on Linux
pub fn sanitize_path(path: &[u8]) -> Cow<[u8]> {
    match path.iter().position(|c| valid_byte(*c)) {
        None => Cow::Borrowed(path),
        Some(i) => {
            let mut p = path[..i].to_vec();
            p.extend(path[i..].iter().cloned().filter(|c| valid_byte(*c)));

            Cow::Owned(p)
        }
    }
}

/// Try to find a suitable default default download directory. The Path is not guaranteed to
/// exist, but will be an absolute path.
#[cfg(target_os = "linux")]
pub fn download_dir() -> Option<path::PathBuf> {
    // $XDG_DOWNLOAD_DIR, $HOME/{D,d}ownload{,s}
    if let Some(d) = env::var_os("XDG_DOWNLOAD_DIR") {
        let p = path::PathBuf::from(d);
        if p.is_absolute() {
            return Some(p);
        }
    }

    let mut home_dir = match env::home_dir() {
        Some(p) => if p.is_absolute() {
            p
        } else {
            return None;
        },
        None => return None,
    };

    match fs::metadata(&home_dir) {
        Ok(ref m) if m.is_dir() => (),
        _ => {
            home_dir.push(DOWNLOAD_SUFFIX);
            return Some(home_dir);
        }
    }

    for d in &[DOWNLOAD_SUFFIX, "downloads", "Download", "download"] {
        let dir = home_dir.join(d);

        match fs::metadata(&dir) {
            Ok(ref m) if m.is_dir() => return Some(dir),
            _ => (),
        }
    }

    // default to home_dir + "Downloads"
    home_dir.push(DOWNLOAD_SUFFIX);

    Some(home_dir)
}

#[cfg(target_os = "macos")]
pub fn download_dir() -> Option<path::PathBuf> {
    // $HOME/Downloads
    // TODO - Use `NSSearchPathForDirectoriesInDomains`
    // https://developer.apple.com/library/ios/documentation/Cocoa/Reference/Foundation/Miscellaneous/Foundation_Functions/index.html#//apple_ref/doc/uid/20000055-181040

    match env::home_dir() {
        Some(mut p) => if p.is_absolute() {
            p.push(DOWNLOAD_SUFFIX);
            Some(p)
        } else {
            None
        },
        _ => None,
    }
}

#[cfg(target_os = "windows")]
pub fn download_dir() -> Option<path::PathBuf> {
    // SHGetKnownFolderPath(FODLERID_Downloads), $HOME/Downloads
    unsafe {
        let mut path = ptr::null_mut();
        let result =
            shell32::SHGetKnownFolderPath(&uuid::FOLDERID_Downloads, 0, ptr::null_mut(), &mut path);

        if result == winapi::winerror::S_OK {
            let mut len = 0;
            let mut path_end = path.clone();

            while *path_end != 0 {
                len += 1;
                path_end = path_end.offset(1);
            }
            let s = String::from_utf16(slice::from_raw_parts(path, len));

            ole32::CoTaskMemFree(mem::transmute(path));

            if let Ok(p) = s {
                return Some(path::PathBuf::from(p));
            }
        }
    }

    match env::home_dir() {
        Some(mut p) => if p.is_absolute() {
            p.push(DOWNLOAD_SUFFIX);
            Some(p)
        } else {
            None
        },
        _ => None,
    }
}
