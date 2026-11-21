mod archive;
mod digest;
mod kind;

pub use archive::{ArchiveEntry, ArchiveError, is_junk, list_7z, list_zip};
pub use digest::{QUICK_SPAN, crc32, quick_fingerprint};
pub use kind::{FileKind, HEADER_LEN, detect_kind, guess_kind};
