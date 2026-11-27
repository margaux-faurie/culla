//! Finds and reads the largest embedded JPEG of a NEF (US-03), using the core IFD walker.
//!
//! The file is opened read-only and read with positioned reads through [`FileSource`], which
//! keeps the first [`HEAD_BYTES`] in memory (one sequential read covers every IFD of a Z6III
//! NEF) and a small window for the few reads elsewhere (JPEG frame headers and tails).
//! A walk reads about 0.5 MB of a 17–20 MB NEF; the preview itself is read by
//! [`read_preview_bytes`], capped in size.

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use culla_core::ifd::{self, ByteRange, Candidate, Limits, ReadAt, WalkError};

/// Bytes read at open. The Z6III's last SubIFD ends near 250 KB (docs/research/z6iii-nef-layout.md).
pub const HEAD_BYTES: u64 = 512 * 1024;
/// Size of the read-ahead window for reads past the head.
pub const WINDOW_BYTES: u64 = 64 * 1024;
/// Default cap for [`read_preview_bytes`], the same as the walker's [`Limits::max_preview_bytes`].
pub const MAX_PREVIEW_BYTES: u64 = Limits::DEFAULT.max_preview_bytes;

/// A file read through positioned reads, with a head buffer and one read-ahead window.
pub struct FileSource {
    file: File,
    len: u64,
    head: Vec<u8>,
    window_at: u64,
    window: Vec<u8>,
    disk_reads: u32,
    disk_bytes: u64,
}

impl FileSource {
    /// Opens `path` read-only and reads its head.
    ///
    /// A plain open: it follows reparse points and hydrates cloud placeholders. Pass only
    /// paths from the current scan, which refuses both (threat model T2, T2b); the open
    /// itself gets those checks when the reader is wired to the CLI or `culla://`.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut source = Self {
            file,
            len,
            head: Vec::new(),
            window_at: 0,
            window: Vec::new(),
            disk_reads: 0,
            disk_bytes: 0,
        };
        let mut head = vec![0u8; len.min(HEAD_BYTES) as usize];
        source.pread(0, &mut head)?;
        source.head = head;
        Ok(source)
    }

    /// Reads from the device so far: (number of reads, bytes). For benchmarks and tests.
    pub fn disk_reads(&self) -> (u32, u64) {
        (self.disk_reads, self.disk_bytes)
    }

    fn pread(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.disk_reads = self.disk_reads.saturating_add(1);
        self.disk_bytes = self.disk_bytes.saturating_add(buf.len() as u64);
        read_exact_at(&self.file, offset, buf)
    }
}

/// Copies `offset..offset + buf.len()` out of `data` (which starts at `data_at`) if it is there.
fn copy_from(data: &[u8], data_at: u64, offset: u64, buf: &mut [u8]) -> bool {
    let Some(start) = offset.checked_sub(data_at) else {
        return false;
    };
    let Ok(start) = usize::try_from(start) else {
        return false;
    };
    match start
        .checked_add(buf.len())
        .and_then(|end| data.get(start..end))
    {
        Some(src) => {
            buf.copy_from_slice(src);
            true
        }
        None => false,
    }
}

impl ReadAt for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset
            .checked_add(buf.len() as u64)
            .filter(|&end| end <= self.len)
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if copy_from(&self.head, 0, offset, buf)
            || copy_from(&self.window, self.window_at, offset, buf)
        {
            return Ok(());
        }
        if buf.len() as u64 > WINDOW_BYTES / 2 {
            return self.pread(offset, buf);
        }
        // Fill the window from `offset`, so forward scans (JPEG segments) stay inside it.
        let at = offset;
        let n = (self.len - at).min(WINDOW_BYTES).max(end - at);
        let mut window = std::mem::take(&mut self.window);
        window.resize(n as usize, 0);
        let filled = self.pread(at, &mut window);
        self.window = window;
        if let Err(e) = filled {
            self.window.clear();
            return Err(e);
        }
        self.window_at = at;
        if copy_from(&self.window, self.window_at, offset, buf) {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::UnexpectedEof))
        }
    }
}

#[cfg(unix)]
fn read_exact_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Ok(n) => {
                let rest = buf;
                buf = &mut rest[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn read_exact_at(_: &File, _: u64, _: &mut [u8]) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

/// Why a preview could not be found or read.
#[derive(Debug)]
pub enum PreviewError {
    /// Opening or reading the file failed.
    Io(io::Error),
    /// The walk failed or found no usable JPEG.
    Walk(WalkError),
    /// The range is larger than the cap.
    TooLarge { length: u64, cap: u64 },
    /// The bytes read are not the JPEG the walk found (the file changed in between).
    Changed,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read the file: {e}"),
            Self::Walk(e) => write!(f, "no preview: {e}"),
            Self::TooLarge { length, cap } => {
                write!(
                    f,
                    "embedded preview of {length} bytes is over the {cap}-byte cap"
                )
            }
            Self::Changed => f.write_str("the file changed while its preview was read"),
        }
    }
}

impl std::error::Error for PreviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Walk(e) => Some(e),
            Self::TooLarge { .. } | Self::Changed => None,
        }
    }
}

impl From<io::Error> for PreviewError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<WalkError> for PreviewError {
    fn from(e: WalkError) -> Self {
        match e {
            WalkError::Read(e) => Self::Io(e),
            e => Self::Walk(e),
        }
    }
}

/// The largest valid embedded JPEG of the NEF at `path`.
pub fn find_preview(path: &Path) -> Result<Candidate, PreviewError> {
    let mut source = FileSource::open(path)?;
    Ok(ifd::find_preview(&mut source, &Limits::DEFAULT)?)
}

/// Reads `range` of `path` (a range from [`find_preview`]) into memory, refusing more than
/// `cap` bytes, and checks it still starts with SOI and ends with EOI. The same file may
/// hold a different JPEG by then, so the decoder re-reads the frame header of these bytes.
/// Same path rule as [`FileSource::open`].
pub fn read_preview_bytes(
    path: &Path,
    range: ByteRange,
    cap: u64,
) -> Result<Vec<u8>, PreviewError> {
    if range.length > cap {
        return Err(PreviewError::TooLarge {
            length: range.length,
            cap,
        });
    }
    let length = usize::try_from(range.length).map_err(|_| PreviewError::TooLarge {
        length: range.length,
        cap,
    })?;
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if range.end().is_none_or(|end| end > file_len) || length < 4 {
        return Err(PreviewError::Changed);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
    bytes.resize(length, 0);
    read_exact_at(&file, range.offset, &mut bytes)?;
    if bytes[..2] != [0xFF, 0xD8] || bytes[length - 2..] != [0xFF, 0xD9] {
        return Err(PreviewError::Changed);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_from_bounds() {
        let data = [1u8, 2, 3, 4];
        let mut b = [0u8; 2];
        assert!(copy_from(&data, 10, 12, &mut b));
        assert_eq!(b, [3, 4]);
        assert!(!copy_from(&data, 10, 13, &mut b));
        assert!(!copy_from(&data, 10, 9, &mut b));
        assert!(!copy_from(&data, 10, u64::MAX, &mut b));
    }
}
