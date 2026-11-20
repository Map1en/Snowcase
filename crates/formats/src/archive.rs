use std::io::{Read, Seek};

use encoding_rs::SHIFT_JIS;
use zip::ZipArchive;
use zip::result::ZipError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    pub crc32: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read the zip directory: {0}")]
pub struct ArchiveError(#[from] ZipError);

pub fn list_zip(reader: impl Read + Seek) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let archive = ZipArchive::new(reader)?;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index_data(index)?;
        if entry.is_dir() {
            continue;
        }
        entries.push(ArchiveEntry {
            path: decode_name(entry.name_raw()).replace('\\', "/"),
            size: entry.size(),
            crc32: entry.crc32(),
        });
    }
    Ok(entries)
}

fn decode_name(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(text) => text.to_owned(),
        Err(_) => SHIFT_JIS.decode_without_bom_handling(raw).0.into_owned(),
    }
}

pub fn is_junk(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|part| part == "__MACOSX" || part.starts_with("._"))
}

#[cfg(test)]
mod tests;
