mod archive;
mod kind;

pub use archive::{ArchiveEntry, ArchiveError, is_junk, list_7z, list_zip};
pub use kind::{FileKind, HEADER_LEN, detect_kind, guess_kind};
