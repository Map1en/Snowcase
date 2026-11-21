use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use snowcase_formats::{FileKind, HEADER_LEN, detect_kind};
use snowcase_store::{
    Blob, BlobId, BlobKind, BlobLocation, Catalog, Facet, FacetKey, FacetNode, FacetOrigin, FacetValue,
    FileImport, Folder, FolderId, Item, ItemChanges, ItemFile, ItemId, Store, StoreError, Tag,
    TagId,
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

pub struct FileView<'a> {
    pub file: &'a ItemFile,
    pub blob: &'a Blob,
    pub locations: Vec<&'a BlobLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFacet {
    pub value: String,
    pub target_item_id: Option<ItemId>,
    pub origin: FacetOrigin,
    pub from: FacetNode,
}

pub fn item_node(item_id: ItemId) -> FacetNode {
    FacetNode {
        item_id,
        blob_id: None,
        inner_path: None,
    }
}

fn parent_node(node: &FacetNode) -> Option<FacetNode> {
    let parent = |blob_id, inner_path| FacetNode {
        item_id: node.item_id,
        blob_id,
        inner_path,
    };
    match (&node.inner_path, node.blob_id) {
        (Some(path), blob_id) => {
            let cut = path.rfind('/').map(|ix| {
                if path[..ix].ends_with('!') {
                    ix - 1
                } else {
                    ix
                }
            });
            match cut.filter(|ix| *ix > 0) {
                Some(ix) => Some(parent(blob_id, Some(path[..ix].to_owned()))),
                None => Some(parent(blob_id, None)),
            }
        }
        (None, Some(_)) => Some(parent(None, None)),
        (None, None) => None,
    }
}

pub struct FolderRow<'a> {
    pub folder: &'a Folder,
    pub depth: usize,
    pub count: usize,
}

#[derive(Clone)]
pub struct Snapshot {
    catalog: Catalog,
    facts: HashMap<ItemId, ItemFacts>,
}

pub struct Library {
    store: Store,
    snapshot: Arc<Snapshot>,
}

impl Library {
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let mut store = Store::open(dir)?;
        let mut catalog = store.load()?;
        if catalog.folders.is_empty() {
            store.create_folder(None, DEFAULT_FOLDER)?;
            catalog = store.load()?;
        }
        Ok(Self {
            store,
            snapshot: Arc::new(Snapshot::new(catalog)),
        })
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.clone()
    }

    pub fn import(&mut self, files: Vec<FileInfo>) -> Result<usize, StoreError> {
        let mut known: BTreeSet<String> = self
            .snapshot
            .catalog
            .locations
            .iter()
            .filter(|location| location.root_id.is_none())
            .map(|location| location.rel_path.clone())
            .collect();
        let fresh: Vec<FileInfo> = files
            .into_iter()
            .filter(|file| known.insert(file.path.clone()))
            .collect();
        let added = fresh.len();
        let ids = self.store.transaction(|store| {
            fresh
                .into_iter()
                .map(|file| import_file(store, file))
                .collect::<Result<Vec<_>, _>>()
        })?;
        self.refresh_items(&ids)?;
        Ok(added)
    }

    pub fn update_item(&mut self, id: ItemId, changes: &ItemChanges) -> Result<(), StoreError> {
        self.store.update_item(id, changes)?;
        self.refresh_items(&[id])
    }

    pub fn tag_item(&mut self, id: ItemId, name: &str) -> Result<Option<TagId>, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let tag = self.store.tag_item(id, name)?;
        let tags = self.store.load_tags()?;
        Arc::make_mut(&mut self.snapshot).catalog.tags = tags;
        self.refresh_items(&[id])?;
        Ok(Some(tag))
    }

    pub fn untag_item(&mut self, id: ItemId, tag: TagId) -> Result<(), StoreError> {
        self.store.untag_item(id, tag)?;
        self.refresh_items(&[id])
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
        if let Some(existing) = self.snapshot.catalog.folders.iter().find(|folder| {
            folder.parent_id == parent && folder.name.to_lowercase() == name.to_lowercase()
        }) {
            return Ok(Some(existing.id));
        }
        let folder = self.store.create_folder(parent, name)?;
        let folders = self.store.load_folders()?;
        Arc::make_mut(&mut self.snapshot).catalog.folders = folders;
        Ok(Some(folder))
    }

    pub fn put_in_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.put_in_folder(id, folder)?;
        self.refresh_items(&[id])
    }

    pub fn remove_from_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.remove_from_folder(id, folder)?;
        self.refresh_items(&[id])
    }

    pub fn set_auto_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        self.write_facets(node, key, FacetOrigin::Auto, values)
    }

    pub fn set_user_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        self.write_facets(node, key, FacetOrigin::User, values)
    }

    pub fn remove_facet(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        value: &str,
    ) -> Result<(), StoreError> {
        self.store.remove_facet(node, key, value)?;
        self.refresh_items(&[node.item_id])
    }

    fn write_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        origin: FacetOrigin,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        assert!(
            key.multi_valued() || values.len() <= 1,
            "{key} takes a single value, got {}",
            values.len()
        );
        self.store.replace_facets(node, key, origin, values)?;
        self.refresh_items(&[node.item_id])
    }

    fn refresh_items(&mut self, ids: &[ItemId]) -> Result<(), StoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self.store.load_items(ids)?;
        Arc::make_mut(&mut self.snapshot).apply_items(ids, rows);
        Ok(())
    }
}

impl Snapshot {
    fn new(catalog: Catalog) -> Self {
        let facts = facts_of(&catalog, |_| true);
        Self { catalog, facts }
    }

    fn apply_items(&mut self, ids: &[ItemId], rows: Catalog) {
        let ids: HashSet<ItemId> = ids.iter().copied().collect();
        let catalog = &mut self.catalog;
        catalog.items.retain(|item| !ids.contains(&item.id));
        catalog.sources.retain(|source| !ids.contains(&source.item_id));
        catalog.files.retain(|file| !ids.contains(&file.item_id));
        catalog.facets.retain(|facet| !ids.contains(&facet.item_id));
        catalog.images.retain(|image| !ids.contains(&image.item_id));
        catalog.item_tags.retain(|link| !ids.contains(&link.item_id));
        catalog.item_folders.retain(|link| !ids.contains(&link.item_id));
        upsert(&mut catalog.items, rows.items, |item| item.id);
        upsert(&mut catalog.sources, rows.sources, |source| source.id);
        upsert(&mut catalog.files, rows.files, |file| (file.item_id, file.blob_id));
        upsert(&mut catalog.facets, rows.facets, |facet| facet.id);
        let fetched: HashSet<BlobId> = rows.blobs.iter().map(|blob| blob.id).collect();
        catalog.locations.retain(|location| !fetched.contains(&location.blob_id));
        upsert(&mut catalog.blobs, rows.blobs, |blob| blob.id);
        upsert(&mut catalog.locations, rows.locations, |location| location.id);
        let referenced: HashSet<BlobId> = catalog.files.iter().map(|file| file.blob_id).collect();
        catalog.blobs.retain(|blob| referenced.contains(&blob.id));
        catalog.locations.retain(|location| referenced.contains(&location.blob_id));
        upsert(&mut catalog.images, rows.images, |image| (image.item_id, image.position));
        upsert(&mut catalog.item_tags, rows.item_tags, |link| (link.item_id, link.tag_id));
        upsert(&mut catalog.item_folders, rows.item_folders, |link| (link.item_id, link.folder_id));
        self.facts.retain(|id, _| !ids.contains(id));
        self.facts
            .extend(facts_of(&self.catalog, |id| ids.contains(&id)));
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
            library: &'a Snapshot,
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

    pub fn files(&self, id: ItemId) -> Vec<FileView<'_>> {
        let catalog = &self.catalog;
        let mut files: Vec<FileView<'_>> = catalog
            .files
            .iter()
            .filter(|file| file.item_id == id)
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
            .collect();
        files.sort_by_key(|view| std::cmp::Reverse((view.file.added_at, view.file.blob_id)));
        files
    }

    pub fn location_path(&self, location: &BlobLocation) -> String {
        match location.root_id {
            None => location.rel_path.clone(),
            Some(root_id) => {
                let root = self
                    .catalog
                    .roots
                    .iter()
                    .find(|root| root.id == root_id)
                    .map_or("", |root| root.path.as_str());
                Path::new(root).join(&location.rel_path).to_string_lossy().into_owned()
            }
        }
    }

    pub fn facets(&self, node: &FacetNode, key: FacetKey) -> Vec<ResolvedFacet> {
        let rows: Vec<&Facet> = self
            .catalog
            .facets
            .iter()
            .filter(|facet| facet.item_id == node.item_id && facet.key == key)
            .collect();
        resolve(&rows, node)
    }
}

fn resolve(rows: &[&Facet], node: &FacetNode) -> Vec<ResolvedFacet> {
    let here: Vec<&Facet> = rows
        .iter()
        .copied()
        .filter(|facet| facet.blob_id == node.blob_id && facet.inner_path == node.inner_path)
        .collect();
    let removed: HashSet<&str> = here
        .iter()
        .filter(|facet| facet.removed)
        .map(|facet| facet.value.as_str())
        .collect();
    let of = |origin| {
        here.iter()
            .filter(move |facet| !facet.removed && facet.origin == origin)
            .map(|facet| ResolvedFacet {
                value: facet.value.clone(),
                target_item_id: facet.target_item_id,
                origin,
                from: node.clone(),
            })
            .collect::<Vec<_>>()
    };
    let user = of(FacetOrigin::User);
    if !user.is_empty() {
        return user;
    }
    let auto = of(FacetOrigin::Auto);
    if !auto.is_empty() {
        return auto;
    }
    parent_node(node)
        .map(|parent| resolve(rows, &parent))
        .unwrap_or_default()
        .into_iter()
        .filter(|facet| !removed.contains(facet.value.as_str()))
        .collect()
}

impl Deref for Library {
    type Target = Snapshot;

    fn deref(&self) -> &Snapshot {
        &self.snapshot
    }
}

fn facts_of(catalog: &Catalog, wanted: impl Fn(ItemId) -> bool) -> HashMap<ItemId, ItemFacts> {
    let blobs: HashMap<_, _> = catalog.blobs.iter().map(|blob| (blob.id, blob)).collect();
    let mut facts: HashMap<ItemId, ItemFacts> = catalog
        .items
        .iter()
        .filter(|item| wanted(item.id))
        .map(|item| (item.id, ItemFacts::default()))
        .collect();
    let mut counted = BTreeSet::new();
    for file in &catalog.files {
        let item = file.item_id;
        let Some(entry) = facts.get_mut(&item) else {
            continue;
        };
        entry.file_count += 1;
        entry.file_names.push(file.file_name.clone());
        if let Some(blob) = blobs.get(&file.blob_id) {
            entry.kinds.insert(FileKind::from(blob.kind));
            if counted.insert((item, blob.id)) {
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
    facts
}

fn upsert<T, K: Ord>(rows: &mut Vec<T>, changed: Vec<T>, key: impl Fn(&T) -> K) {
    for row in changed {
        match rows.binary_search_by_key(&key(&row), &key) {
            Ok(ix) => rows[ix] = row,
            Err(ix) => rows.insert(ix, row),
        }
    }
}

fn import_file(store: &mut Store, file: FileInfo) -> Result<ItemId, StoreError> {
    let item = store.create_item(&title_of(&file.name))?;
    store.add_file(&FileImport {
        item_id: item,
        root_id: None,
        rel_path: file.path,
        file_name: file.name,
        display_name: None,
        size: i64::try_from(file.size).unwrap_or(i64::MAX),
        modified_ms: file.modified_ms,
        fingerprint: format!("{}:{}", file.size, file.modified_ms),
        crc32: None,
        kind: BlobKind::from(file.kind),
    })?;
    Ok(item)
}

fn title_of(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| file_name.to_owned())
}

mod scanning;

pub use scanning::ScanReport;

#[cfg(test)]
mod tests;
