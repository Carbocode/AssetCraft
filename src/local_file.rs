//! Files and lossless path storage for the platform running AssetCraft.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, ffi::OsStringExt, fs::OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::{ffi::OsStrExt, ffi::OsStringExt, fs::OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Store native path bytes. Unix bytes match the public version 6 catalog format.
pub fn encode_path(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
}

/// Read a native absolute path, rejecting embedded nulls before filesystem access.
pub fn decode_path(bytes: Vec<u8>) -> Option<PathBuf> {
    #[cfg(unix)]
    let path = {
        if bytes.contains(&0) {
            return None;
        }
        PathBuf::from(OsString::from_vec(bytes))
    };
    #[cfg(windows)]
    let path = {
        let (pairs, remainder) = bytes.as_chunks::<2>();
        let units: Vec<_> = pairs.iter().copied().map(u16::from_le_bytes).collect();
        if !remainder.is_empty() || units.contains(&0) {
            return None;
        }
        PathBuf::from(OsString::from_wide(&units))
    };
    path.is_absolute().then_some(path)
}

/// Open an original so its type can be checked before reading any content.
pub fn open_read(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    // O_NONBLOCK lets us inspect a file replaced with a FIFO without waiting for a writer.
    #[cfg(target_os = "linux")]
    options.custom_flags(0o4000);
    #[cfg(target_os = "macos")]
    options.custom_flags(0x0004);
    // FILE_SHARE_READ prevents writes and replacement while the original is open.
    #[cfg(windows)]
    options.share_mode(0x0000_0001);
    options.open(path)
}
