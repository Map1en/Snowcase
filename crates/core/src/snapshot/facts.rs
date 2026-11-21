use std::collections::{BTreeSet, HashMap};

use snowcase_formats::FileKind;
use snowcase_store::{BlobId, Catalog, Facet, FacetKey, FolderId, ItemId, RootId, TagId};

use super::{item_node, resolve};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemFacts {
    pub all_tags: BTreeSet<TagId>,
    pub folders: BTreeSet<FolderId>,
    pub size: i64,
    pub file_count: usize,
    pub file_names: Vec<String>,
    pub kinds: BTreeSet<FileKind>,
    pub roots: BTreeSet<RootId>,
    pub attributes: Vec<(FacetKey, String)>,
    pub shop: Option<String>,
}

impl ItemFacts {
    pub fn attribute(&self, key: FacetKey) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn has(&self, key: FacetKey, value: &str) -> bool {
        self.attributes
            .iter()
            .any(|(candidate, found)| *candidate == key && found == value)
    }
}

pub(crate) fn facts_of(catalog: &Catalog, wanted: impl Fn(ItemId) -> bool) -> HashMap<ItemId, ItemFacts> {
    let blobs: HashMap<_, _> = catalog.blobs.iter().map(|blob| (blob.id, blob)).collect();
    let mut facts: HashMap<ItemId, ItemFacts> = catalog
        .items
        .iter()
        .filter(|item| wanted(item.id))
        .map(|item| (item.id, ItemFacts::default()))
        .collect();
    let mut roots: HashMap<BlobId, Vec<RootId>> = HashMap::new();
    for location in &catalog.locations {
        if let Some(root) = location.root_id {
            roots.entry(location.blob_id).or_default().push(root);
        }
    }
    let mut counted = BTreeSet::new();
    for file in &catalog.files {
        let item = file.item_id;
        let Some(entry) = facts.get_mut(&item) else {
            continue;
        };
        entry.file_count += 1;
        entry.file_names.push(file.file_name.clone());
        if let Some(name) = &file.display_name {
            entry.file_names.push(name.clone());
        }
        entry.roots.extend(roots.get(&file.blob_id).into_iter().flatten());
        if let Some(blob) = blobs.get(&file.blob_id) {
            entry.kinds.insert(FileKind::from(blob.kind));
            if counted.insert((item, blob.id)) {
                entry.size += blob.size;
            }
        }
    }
    for link in &catalog.node_tags {
        if let Some(entry) = facts.get_mut(&link.item_id) {
            entry.all_tags.insert(link.tag_id);
        }
    }
    for link in &catalog.item_folders {
        if let Some(entry) = facts.get_mut(&link.item_id) {
            entry.folders.insert(link.folder_id);
        }
    }
    for source in &catalog.sources {
        if let (Some(entry), Some(author)) = (facts.get_mut(&source.item_id), &source.author) {
            entry.shop.get_or_insert_with(|| author.clone());
        }
    }
    let mut by_item: HashMap<ItemId, Vec<&Facet>> = HashMap::new();
    for facet in &catalog.facets {
        by_item.entry(facet.item_id).or_default().push(facet);
    }
    for (id, entry) in &mut facts {
        let Some(rows) = by_item.get(id) else {
            continue;
        };
        let node = item_node(*id);
        for key in FacetKey::ALL {
            let keyed: Vec<&Facet> = rows.iter().copied().filter(|facet| facet.key == *key).collect();
            entry
                .attributes
                .extend(resolve(&keyed, &node).into_iter().map(|facet| (*key, facet.value)));
        }
        if entry.shop.is_none() {
            entry.shop = entry.attribute(FacetKey::Shop).map(str::to_owned);
        }
    }
    facts
}

pub(crate) fn tally(values: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}
