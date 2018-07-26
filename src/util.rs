use std::borrow::Cow;
use std::path::PathBuf;

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
crate fn sanitize_path(path: &[u8]) -> Cow<'_, [u8]> {
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
crate fn download_dir() -> Option<PathBuf> {
    dirs::download_dir().filter(|p| p.is_absolute())
}
