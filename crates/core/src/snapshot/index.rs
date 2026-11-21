use std::collections::HashMap;
use std::sync::Arc;

use snowcase_store::{Blob, BlobId, Item, ItemFile, ItemId, Tag, TagId};

use super::{PackageSummary, Snapshot};

#[derive(Clone, Default)]
pub(crate) struct Index {
    pub(crate) locations: HashMap<BlobId, Vec<usize>>,
    pub(crate) node_tags: HashMap<ItemId, Vec<usize>>,
    pub(crate) members: HashMap<String, Vec<(String, Arc<PackageSummary>)>>,
    pub(crate) titles: HashMap<String, Vec<ItemId>>,
    pub(crate) prints: HashMap<String, Vec<(ItemId, BlobId)>>,
    pub(crate) roots: HashMap<String, Vec<ItemId>>,
}

fn find<'a, T, K: Ord>(rows: &'a [T], key: &K, of: impl Fn(&T) -> K) -> Option<&'a T> {
    rows.binary_search_by(|row| of(row).cmp(key)).ok().map(|ix| &rows[ix])
}

impl Snapshot {
    pub(crate) fn reindex(&mut self) {
        let catalog = &self.catalog;
        let mut index = Index::default();
        for (ix, location) in catalog.locations.iter().enumerate() {
            index.locations.entry(location.blob_id).or_default().push(ix);
        }
        for (ix, link) in catalog.node_tags.iter().enumerate() {
            index.node_tags.entry(link.item_id).or_default().push(ix);
        }
        for (key, summary) in self.packages.iter().filter(|(key, _)| !key.member.is_empty()) {
            index
                .members
                .entry(key.fingerprint.clone())
                .or_default()
                .push((key.member.clone(), summary.clone()));
        }
        for members in index.members.values_mut() {
            members.sort_by(|a, b| a.0.cmp(&b.0));
        }
        for item in &catalog.items {
            index.titles.entry(item.title.to_lowercase()).or_default().push(item.id);
        }
        for file in &catalog.files {
            if let Some(print) = find(&catalog.blobs, &file.blob_id, |blob| blob.id).and_then(|blob| blob.fingerprint.clone()) {
                index.prints.entry(print).or_default().push((file.item_id, file.blob_id));
            }
        }
        self.index = index;
        let mut roots: HashMap<String, Vec<ItemId>> = HashMap::new();
        for item in &self.catalog.items {
            for root in self.package_roots(item.id) {
                roots.entry(root.to_owned()).or_default().push(item.id);
            }
        }
        self.index.roots = roots;
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        find(&self.catalog.items, &id, |item| item.id)
    }

    pub(crate) fn blob(&self, id: BlobId) -> Option<&Blob> {
        find(&self.catalog.blobs, &id, |blob| blob.id)
    }

    pub(crate) fn tag(&self, id: TagId) -> Option<&Tag> {
        find(&self.catalog.tags, &id, |tag| tag.id)
    }

    pub(crate) fn item_files(&self, id: ItemId) -> &[ItemFile] {
        let files = &self.catalog.files;
        let start = files.partition_point(|file| file.item_id < id);
        let end = files.partition_point(|file| file.item_id <= id);
        &files[start..end]
    }

    pub(crate) fn item_file(&self, id: ItemId, blob: BlobId) -> Option<&ItemFile> {
        find(&self.catalog.files, &(id, blob), |file| (file.item_id, file.blob_id))
    }
}
