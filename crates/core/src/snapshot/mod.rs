use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use snowcase_store::{Blob, BlobId, BlobLocation, Catalog, Facet, FacetKey, FacetNode, FacetOrigin, Folder, FolderId, Item, ItemFile, ItemId, NodeTag, PackageKey, Root, RootId, StoreKind, Tag};

use facts::facts_of;

mod facts;
mod hints;
mod index;
mod licenses;
mod packages;
mod query;

pub use facts::ItemFacts;
pub use hints::Hints;
pub(crate) use licenses::{Licenses, record};
pub use packages::PackageSummary;
pub use query::{Filter, Hit, Scope, SortKey};

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
    pub(crate) catalog: Catalog,
    pub(crate) facts: HashMap<ItemId, ItemFacts>,
    pub(crate) packages: Arc<HashMap<PackageKey, Arc<PackageSummary>>>,
    pub(crate) licenses: Arc<licenses::Licenses>,
    pub(crate) media_dir: PathBuf,
    index: index::Index,
}

impl Snapshot {
    pub(crate) fn new(catalog: Catalog) -> Self {
        let facts = facts_of(&catalog, |_| true);
        Self {
            catalog,
            facts,
            packages: Arc::default(),
            licenses: Arc::default(),
            media_dir: PathBuf::new(),
            index: index::Index::default(),
        }
    }

    pub(crate) fn apply_items(&mut self, ids: &[ItemId], rows: Catalog) {
        let ids: HashSet<ItemId> = ids.iter().copied().collect();
        let catalog = &mut self.catalog;
        catalog.items.retain(|item| !ids.contains(&item.id));
        catalog.sources.retain(|source| !ids.contains(&source.item_id));
        catalog.files.retain(|file| !ids.contains(&file.item_id));
        catalog.facets.retain(|facet| !ids.contains(&facet.item_id));
        catalog.images.retain(|image| !ids.contains(&image.item_id));
        catalog.node_tags.retain(|link| !ids.contains(&link.item_id));
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
        upsert(&mut catalog.node_tags, rows.node_tags, |link| link.id);
        upsert(&mut catalog.item_folders, rows.item_folders, |link| (link.item_id, link.folder_id));
        self.facts.retain(|id, _| !ids.contains(id));
        self.facts
            .extend(facts_of(&self.catalog, |id| ids.contains(&id)));
        self.reindex();
    }

    pub fn items(&self) -> &[Item] {
        &self.catalog.items
    }

    pub fn facts(&self, id: ItemId) -> Option<&ItemFacts> {
        self.facts.get(&id)
    }

    pub fn cover_path(&self, id: ItemId) -> Option<PathBuf> {
        let hash = self.item(id)?.cover_media.as_deref()?;
        let media = self.catalog.media.iter().find(|media| media.hash == hash)?;
        Some(media_path(&self.media_dir, hash, &media.mime))
    }

    pub fn booth_page(&self, id: ItemId) -> Option<(&str, &str)> {
        self.catalog
            .sources
            .iter()
            .find(|source| source.item_id == id && source.store == StoreKind::Booth)
            .and_then(|source| Some((source.url.as_deref()?, source.title.as_deref().unwrap_or_default())))
    }

    pub(crate) fn node_tags_of(&self, id: ItemId) -> impl Iterator<Item = &NodeTag> {
        self.index
            .node_tags
            .get(&id)
            .into_iter()
            .flatten()
            .map(|ix| &self.catalog.node_tags[*ix])
    }

    pub fn item_tags(&self, id: ItemId) -> Vec<&Tag> {
        let mut tags: Vec<&Tag> = self
            .node_tags_of(id)
            .filter(|link| link.blob_id.is_none())
            .filter_map(|link| self.tag(link.tag_id))
            .collect();
        tags.sort_by_key(|tag| tag.id);
        tags
    }

    pub fn file_tags(&self, id: ItemId, blob: BlobId) -> Vec<&Tag> {
        self.node_tags_of(id)
            .filter(|link| link.blob_id == Some(blob))
            .filter_map(|link| self.tag(link.tag_id))
            .collect()
    }

    pub fn tag_counts(&self) -> Vec<(&Tag, usize)> {
        let mut tags: Vec<_> = self
            .catalog
            .tags
            .iter()
            .map(|tag| {
                let count = self.facts.values().filter(|facts| facts.all_tags.contains(&tag.id)).count();
                (tag, count)
            })
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
        let mut files: Vec<FileView<'_>> = self
            .item_files(id)
            .iter()
            .filter_map(|file| {
                let blob = self.blob(file.blob_id)?;
                Some(FileView {
                    file,
                    blob,
                    locations: self
                        .index
                        .locations
                        .get(&blob.id)
                        .into_iter()
                        .flatten()
                        .map(|ix| &self.catalog.locations[*ix])
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

    pub fn roots(&self) -> Vec<(&Root, usize)> {
        self.catalog
            .roots
            .iter()
            .map(|root| (root, self.count(&Scope::Root(root.id))))
            .collect()
    }

    pub(crate) fn items_in_root(&self, root: RootId) -> Vec<ItemId> {
        self.facts
            .iter()
            .filter(|(_, facts)| facts.roots.contains(&root))
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn only_in_root(&self, root: RootId) -> usize {
        let catalog = &self.catalog;
        let elsewhere: HashSet<BlobId> = catalog
            .locations
            .iter()
            .filter(|location| location.root_id != Some(root))
            .map(|location| location.blob_id)
            .collect();
        self.items_in_root(root)
            .into_iter()
            .filter(|id| self.item_files(*id).iter().all(|file| !elsewhere.contains(&file.blob_id)))
            .count()
    }
}

pub(crate) fn resolve(rows: &[&Facet], node: &FacetNode) -> Vec<ResolvedFacet> {
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

pub(crate) fn media_path(dir: &Path, hash: &str, mime: &str) -> PathBuf {
    let extension = match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    dir.join(&hash[..2]).join(format!("{hash}.{extension}"))
}

pub fn shown_name(file: &ItemFile) -> &str {
    file.display_name.as_deref().unwrap_or(&file.file_name)
}

fn upsert<T, K: Ord>(rows: &mut Vec<T>, changed: Vec<T>, key: impl Fn(&T) -> K) {
    for row in changed {
        match rows.binary_search_by_key(&key(&row), &key) {
            Ok(ix) => rows[ix] = row,
            Err(ix) => rows.insert(ix, row),
        }
    }
}

#[cfg(test)]
mod tests;
