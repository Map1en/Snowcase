use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use snowcase_formats::{FileKind, HEADER_LEN, detect_kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetId(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub id: AssetId,
    pub file: FileInfo,
}

#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("{0} is not a file")]
    NotAFile(PathBuf),
    #[error("failed to read {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

pub fn inspect_file(path: &Path) -> Result<FileInfo, InspectError> {
    let io_error = |source| InspectError::Io {
        path: path.to_path_buf(),
        source,
    };
    let metadata = path.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(InspectError::NotAFile(path.to_path_buf()));
    }
    let mut header = Vec::with_capacity(HEADER_LEN);
    File::open(path)
        .and_then(|file| file.take(HEADER_LEN as u64).read_to_end(&mut header))
        .map_err(io_error)?;
    let extension = path.extension().and_then(|ext| ext.to_str());
    Ok(FileInfo {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: metadata.len(),
        kind: detect_kind(extension, &header),
    })
}

#[derive(Debug, Default)]
pub struct Library {
    assets: Vec<Asset>,
    next_id: u64,
}

impl Library {
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    pub fn insert(&mut self, files: impl IntoIterator<Item = FileInfo>) -> usize {
        let mut added = 0;
        for file in files {
            if self.assets.iter().any(|asset| asset.file.path == file.path) {
                continue;
            }
            self.next_id += 1;
            self.assets.push(Asset {
                id: AssetId(self.next_id),
                file,
            });
            added += 1;
        }
        added
    }
}

#[cfg(test)]
mod tests;
