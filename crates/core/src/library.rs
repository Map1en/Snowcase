use std::collections::{BTreeMap, BTreeSet};
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
    pub tags: BTreeSet<String>,
    pub folder: Option<String>,
}

impl Asset {
    fn matches(&self, scope: &Scope, needle: &str) -> bool {
        let in_scope = match scope {
            Scope::All => true,
            Scope::Untagged => self.tags.is_empty(),
            Scope::Unfiled => self.folder.is_none(),
            Scope::Kind(kind) => self.file.kind == *kind,
            Scope::Tag(tag) => self.tags.contains(tag),
            Scope::Folder(folder) => self.folder.as_ref() == Some(folder),
        };
        in_scope
            && (needle.is_empty()
                || self.file.name.to_lowercase().contains(needle)
                || self.tags.iter().any(|tag| tag.to_lowercase().contains(needle)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Untagged,
    Unfiled,
    Kind(FileKind),
    Tag(String),
    Folder(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Added,
    Name,
    Size,
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
    folders: BTreeSet<String>,
    next_id: u64,
}

impl Library {
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    pub fn get(&self, id: AssetId) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.id == id)
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
                tags: BTreeSet::new(),
                folder: None,
            });
            added += 1;
        }
        added
    }

    pub fn query(&self, scope: &Scope, search: &str, sort: SortKey) -> Vec<&Asset> {
        let needle = search.trim().to_lowercase();
        let mut found: Vec<_> = self
            .assets
            .iter()
            .filter(|asset| asset.matches(scope, &needle))
            .collect();
        match sort {
            SortKey::Added => found.reverse(),
            SortKey::Name => found.sort_by_key(|asset| asset.file.name.to_lowercase()),
            SortKey::Size => found.sort_by_key(|asset| std::cmp::Reverse(asset.file.size)),
        }
        found
    }

    pub fn count(&self, scope: &Scope) -> usize {
        self.assets
            .iter()
            .filter(|asset| asset.matches(scope, ""))
            .count()
    }

    pub fn tags(&self) -> BTreeMap<&str, usize> {
        let mut tags = BTreeMap::new();
        for tag in self.assets.iter().flat_map(|asset| &asset.tags) {
            *tags.entry(tag.as_str()).or_default() += 1;
        }
        tags
    }

    pub fn folders(&self) -> impl Iterator<Item = &str> {
        self.folders.iter().map(String::as_str)
    }

    pub fn add_tag(&mut self, id: AssetId, tag: &str) -> bool {
        let tag = tag.trim();
        match self.asset_mut(id) {
            Some(asset) if !tag.is_empty() => asset.tags.insert(tag.to_owned()),
            _ => false,
        }
    }

    pub fn remove_tag(&mut self, id: AssetId, tag: &str) -> bool {
        self.asset_mut(id).is_some_and(|asset| asset.tags.remove(tag))
    }

    pub fn create_folder(&mut self, name: &str) -> bool {
        let name = name.trim();
        !name.is_empty() && self.folders.insert(name.to_owned())
    }

    pub fn move_to_folder(&mut self, id: AssetId, folder: Option<&str>) -> bool {
        let folder = folder.map(str::trim).filter(|name| !name.is_empty());
        if self.asset_mut(id).is_none() {
            return false;
        }
        if let Some(name) = folder {
            self.create_folder(name);
        }
        if let Some(asset) = self.asset_mut(id) {
            asset.folder = folder.map(str::to_owned);
        }
        true
    }

    fn asset_mut(&mut self, id: AssetId) -> Option<&mut Asset> {
        self.assets.iter_mut().find(|asset| asset.id == id)
    }
}

#[cfg(test)]
mod tests;
