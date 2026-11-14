use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use snowcase_formats::{FileKind, HEADER_LEN, detect_kind};
use snowcase_store::{
    Blob, BlobKind, BlobLocation, Catalog, FileImport, FileRole, Folder, FolderId, Item,
    ItemChanges, ItemId, ItemStatus, Store, StoreError, Tag, TagId, Version, VersionFile,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
    pub kind: FileKind,
}

#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("{0} is not a file")]
    NotAFile(PathBuf),
    #[error("{0} is not a valid UTF-8 path")]
    NonUtf8Path(PathBuf),
    #[error("failed to read {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

pub fn inspect_file(path: &Path) -> Result<FileInfo, InspectError> {
    let io_error = |source| InspectError::Io {
        path: path.to_path_buf(),
        source,
    };
    let text = path
        .to_str()
        .ok_or_else(|| InspectError::NonUtf8Path(path.to_path_buf()))?;
    let metadata = path.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(InspectError::NotAFile(path.to_path_buf()));
    }
    let mut header = Vec::with_capacity(HEADER_LEN);
    File::open(path)
        .and_then(|file| file.take(HEADER_LEN as u64).read_to_end(&mut header))
        .map_err(io_error)?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX));
    Ok(FileInfo {
        path: text.to_owned(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: metadata.len(),
        modified_ms,
        kind: detect_kind(path.extension().and_then(|ext| ext.to_str()), &header),
    })
}

pub const DEFAULT_FOLDER: &str = "Default";

pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending: Vec<PathBuf> = paths.iter().rev().cloned().collect();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            let mut children: Vec<PathBuf> = std::fs::read_dir(&path)
                .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
                .unwrap_or_default();
            children.sort();
            pending.extend(children.into_iter().rev());
        } else {
            files.push(path);
        }
    }
    files
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Inbox,
    InLibrary,
    Tag(TagId),
    Folder(FolderId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Added,
    Name,
    Size,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemFacts {
    pub tags: BTreeSet<TagId>,
    pub folders: BTreeSet<FolderId>,
    pub size: i64,
    pub file_count: usize,
    pub file_names: Vec<String>,
    pub kinds: BTreeSet<FileKind>,
}

pub struct VersionView<'a> {
    pub version: &'a Version,
    pub files: Vec<FileView<'a>>,
}

pub struct FileView<'a> {
    pub file: &'a VersionFile,
    pub blob: &'a Blob,
    pub locations: Vec<&'a BlobLocation>,
}

pub struct FolderRow<'a> {
    pub folder: &'a Folder,
    pub depth: usize,
    pub count: usize,
}

pub struct Library {
    store: Store,
    catalog: Catalog,
    facts: HashMap<ItemId, ItemFacts>,
}

impl Library {
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let mut store = Store::open(dir)?;
        let mut catalog = store.load()?;
        if catalog.folders.is_empty() {
            store.create_folder(None, DEFAULT_FOLDER)?;
            catalog = store.load()?;
        }
        let mut library = Self {
            store,
            catalog,
            facts: HashMap::new(),
        };
        library.index();
        Ok(library)
    }

    pub fn items(&self) -> &[Item] {
        &self.catalog.items
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.catalog.items.iter().find(|item| item.id == id)
    }

    pub fn facts(&self, id: ItemId) -> Option<&ItemFacts> {
        self.facts.get(&id)
    }

    pub fn query(&self, scope: &Scope, search: &str, sort: SortKey) -> Vec<&Item> {
        let needle = search.trim().to_lowercase();
        let mut found: Vec<&Item> = self
            .catalog
            .items
            .iter()
            .filter(|item| self.matches(item, scope, &needle))
            .collect();
        match sort {
            SortKey::Added => found.sort_by_key(|item| std::cmp::Reverse((item.created_at, item.id))),
            SortKey::Name => found.sort_by_key(|item| (item.title.to_lowercase(), item.id)),
            SortKey::Size => found.sort_by_key(|item| {
                std::cmp::Reverse((self.facts.get(&item.id).map_or(0, |facts| facts.size), item.id))
            }),
        }
        found
    }

    pub fn count(&self, scope: &Scope) -> usize {
        self.catalog
            .items
            .iter()
            .filter(|item| self.matches(item, scope, ""))
            .count()
    }

    fn matches(&self, item: &Item, scope: &Scope, needle: &str) -> bool {
        let facts = self.facts.get(&item.id);
        let in_scope = match scope {
            Scope::All => true,
            Scope::Inbox => item.status == ItemStatus::Inbox,
            Scope::InLibrary => item.status == ItemStatus::Confirmed,
            Scope::Tag(tag) => facts.is_some_and(|facts| facts.tags.contains(tag)),
            Scope::Folder(folder) => facts.is_some_and(|facts| facts.folders.contains(folder)),
        };
        if !in_scope {
            return false;
        }
        if needle.is_empty() || item.title.to_lowercase().contains(needle) {
            return true;
        }
        facts.is_some_and(|facts| {
            facts
                .file_names
                .iter()
                .any(|name| name.to_lowercase().contains(needle))
                || facts.tags.iter().any(|id| {
                    self.tag(*id)
                        .is_some_and(|tag| tag.name.to_lowercase().contains(needle))
                })
        })
    }

    fn tag(&self, id: TagId) -> Option<&Tag> {
        self.catalog.tags.iter().find(|tag| tag.id == id)
    }

    pub fn tags_of(&self, id: ItemId) -> Vec<&Tag> {
        self.facts
            .get(&id)
            .map(|facts| facts.tags.iter().filter_map(|tag| self.tag(*tag)).collect())
            .unwrap_or_default()
    }

    pub fn tag_counts(&self) -> Vec<(&Tag, usize)> {
        let mut tags: Vec<_> = self
            .catalog
            .tags
            .iter()
            .map(|tag| (tag, self.count(&Scope::Tag(tag.id))))
            .filter(|(_, count)| *count > 0)
            .collect();
        tags.sort_by_key(|(tag, _)| tag.name.to_lowercase());
        tags
    }

    pub fn folders_of(&self, id: ItemId) -> Vec<&Folder> {
        self.facts
            .get(&id)
            .map(|facts| {
                facts
                    .folders
                    .iter()
                    .filter_map(|folder| self.catalog.folders.iter().find(|f| f.id == *folder))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn folder_tree(&self) -> Vec<FolderRow<'_>> {
        fn walk<'a>(
            library: &'a Library,
            parent: Option<FolderId>,
            depth: usize,
            rows: &mut Vec<FolderRow<'a>>,
        ) {
            let mut children: Vec<_> = library
                .catalog
                .folders
                .iter()
                .filter(|folder| folder.parent_id == parent)
                .collect();
            children.sort_by_key(|folder| (folder.position, folder.id));
            for folder in children {
                rows.push(FolderRow {
                    folder,
                    depth,
                    count: library.count(&Scope::Folder(folder.id)),
                });
                walk(library, Some(folder.id), depth + 1, rows);
            }
        }
        let mut rows = Vec::new();
        walk(self, None, 0, &mut rows);
        rows
    }

    pub fn versions(&self, id: ItemId) -> Vec<VersionView<'_>> {
        let catalog = &self.catalog;
        let mut versions: Vec<_> = catalog
            .versions
            .iter()
            .filter(|version| version.item_id == id)
            .collect();
        versions.sort_by_key(|version| std::cmp::Reverse((version.added_at, version.id)));
        versions
            .into_iter()
            .map(|version| VersionView {
                version,
                files: catalog
                    .version_files
                    .iter()
                    .filter(|file| file.version_id == version.id)
                    .filter_map(|file| {
                        let blob = catalog.blobs.iter().find(|blob| blob.id == file.blob_id)?;
                        Some(FileView {
                            file,
                            blob,
                            locations: catalog
                                .locations
                                .iter()
                                .filter(|location| location.blob_id == blob.id)
                                .collect(),
                        })
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn import(&mut self, files: Vec<FileInfo>) -> Result<usize, StoreError> {
        let mut known: BTreeSet<String> = self
            .catalog
            .locations
            .iter()
            .map(|location| location.path.clone())
            .collect();
        let result = files
            .into_iter()
            .filter(|file| known.insert(file.path.clone()))
            .try_fold(0, |added, file| self.import_file(file).map(|()| added + 1));
        self.reload()?;
        result
    }

    fn import_file(&mut self, file: FileInfo) -> Result<(), StoreError> {
        let item = self
            .store
            .create_item(&title_of(&file.name), ItemStatus::Inbox)?;
        let version = self.store.add_version(item, None, None)?;
        self.store.add_file(&FileImport {
            version_id: version,
            path: file.path,
            file_name: file.name,
            size: i64::try_from(file.size).unwrap_or(i64::MAX),
            fingerprint: format!("{}:{}", file.size, file.modified_ms),
            kind: BlobKind::from(file.kind),
            part: None,
            variant: None,
            role: (file.kind == FileKind::UnityPackage).then_some(FileRole::Package),
        })?;
        Ok(())
    }

    pub fn update_item(&mut self, id: ItemId, changes: &ItemChanges) -> Result<(), StoreError> {
        self.store.update_item(id, changes)?;
        self.reload()
    }

    pub fn tag_item(&mut self, id: ItemId, name: &str) -> Result<Option<TagId>, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let tag = self.store.tag_item(id, name)?;
        self.reload()?;
        Ok(Some(tag))
    }

    pub fn untag_item(&mut self, id: ItemId, tag: TagId) -> Result<(), StoreError> {
        self.store.untag_item(id, tag)?;
        self.reload()
    }

    pub fn create_folder(
        &mut self,
        parent: Option<FolderId>,
        name: &str,
    ) -> Result<Option<FolderId>, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(existing) = self.catalog.folders.iter().find(|folder| {
            folder.parent_id == parent && folder.name.to_lowercase() == name.to_lowercase()
        }) {
            return Ok(Some(existing.id));
        }
        let folder = self.store.create_folder(parent, name)?;
        self.reload()?;
        Ok(Some(folder))
    }

    pub fn put_in_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.put_in_folder(id, folder)?;
        self.reload()
    }

    pub fn remove_from_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.remove_from_folder(id, folder)?;
        self.reload()
    }

    fn reload(&mut self) -> Result<(), StoreError> {
        self.catalog = self.store.load()?;
        self.index();
        Ok(())
    }

    fn index(&mut self) {
        let catalog = &self.catalog;
        let version_items: HashMap<_, _> = catalog
            .versions
            .iter()
            .map(|version| (version.id, version.item_id))
            .collect();
        let blobs: HashMap<_, _> = catalog.blobs.iter().map(|blob| (blob.id, blob)).collect();
        let mut facts: HashMap<ItemId, ItemFacts> = catalog
            .items
            .iter()
            .map(|item| (item.id, ItemFacts::default()))
            .collect();
        let mut counted = BTreeSet::new();
        for file in &catalog.version_files {
            let Some(item) = version_items.get(&file.version_id) else {
                continue;
            };
            let Some(entry) = facts.get_mut(item) else {
                continue;
            };
            entry.file_count += 1;
            entry.file_names.push(file.file_name.clone());
            if let Some(blob) = blobs.get(&file.blob_id) {
                entry.kinds.insert(FileKind::from(blob.kind));
                if counted.insert((*item, blob.id)) {
                    entry.size += blob.size;
                }
            }
        }
        for link in &catalog.item_tags {
            if let Some(entry) = facts.get_mut(&link.item_id) {
                entry.tags.insert(link.tag_id);
            }
        }
        for link in &catalog.item_folders {
            if let Some(entry) = facts.get_mut(&link.item_id) {
                entry.folders.insert(link.folder_id);
            }
        }
        self.facts = facts;
    }
}

fn title_of(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| file_name.to_owned())
}

#[cfg(test)]
mod tests;
