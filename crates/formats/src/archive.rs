use std::io::{Read, Seek};

use encoding_rs::SHIFT_JIS;
use sevenz_rust2::{Archive, Password};
use zip::ZipArchive;

use crate::package::{PackageAsset, list_unitypackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    pub crc32: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("failed to read the zip directory: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("failed to read the 7z directory: {0}")]
    SevenZip(#[from] sevenz_rust2::Error),
}

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

pub fn list_7z(mut reader: impl Read + Seek) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let archive = Archive::read(&mut reader, &Password::empty())?;
    Ok(archive
        .files
        .into_iter()
        .filter(|entry| !entry.is_directory)
        .map(|entry| ArchiveEntry {
            path: entry.name.replace('\\', "/"),
            size: entry.size,
            crc32: entry.crc as u32,
        })
        .collect())
}

pub fn list_zipped_unitypackage(reader: impl Read + Seek, member: &str) -> std::io::Result<Vec<PackageAsset>> {
    with_member(reader, member, |entry| list_unitypackage(entry))
}

pub fn read_zip_member(reader: impl Read + Seek, member: &str, limit: u64) -> std::io::Result<Vec<u8>> {
    with_member(reader, member, |entry| {
        if entry.size() > limit {
            return Err(std::io::Error::other(format!("{member} is larger than {limit} bytes")));
        }
        let mut bytes = Vec::new();
        entry.take(limit).read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn with_member<R: Read + Seek, T>(
    reader: R,
    member: &str,
    read: impl FnOnce(zip::read::ZipFile<'_, R>) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut archive = ZipArchive::new(reader).map_err(std::io::Error::other)?;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(std::io::Error::other)?;
        if decode_name(entry.name_raw()).replace('\\', "/") == member {
            return read(entry);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{member} is not in the archive")))
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
