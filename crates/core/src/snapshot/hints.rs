use std::collections::HashSet;

use snowcase_store::{BlobId, FacetKey, ItemId};

use super::Snapshot;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    pub extracted: usize,
    pub duplicates: usize,
    pub versions: Vec<(ItemId, String)>,
}

impl Snapshot {
    pub fn hints(&self, id: ItemId) -> Hints {
        let own = self.item_files(id);
        let blobs: HashSet<BlobId> = own.iter().map(|file| file.blob_id).collect();
        let prints: HashSet<&str> = own.iter().filter_map(|file| self.fingerprint(file.blob_id)).collect();
        let duplicates = prints
            .iter()
            .filter_map(|print| self.index.prints.get(*print))
            .flatten()
            .filter(|(_, blob)| !blobs.contains(blob))
            .count();
        let title = self.item(id).map(|item| item.title.to_lowercase()).unwrap_or_default();
        let version = self.facts.get(&id).and_then(|facts| facts.attribute(FacetKey::Version));
        let mut versions: Vec<(ItemId, String)> = self
            .index
            .titles
            .get(&title)
            .into_iter()
            .flatten()
            .filter(|other| **other != id)
            .filter_map(|other| {
                let other_version = self.facts.get(other)?.attribute(FacetKey::Version)?;
                (Some(other_version) != version).then(|| (*other, other_version.to_owned()))
            })
            .collect();
        versions.sort_by(|a, b| a.1.cmp(&b.1));
        Hints {
            extracted: own.iter().filter(|file| file.extracted_from_blob_id.is_some()).count(),
            duplicates,
            versions,
        }
    }
}
