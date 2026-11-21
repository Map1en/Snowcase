use std::collections::HashSet;

use snowcase_store::{BlobId, FacetKey, ItemFile, ItemId};

use super::Snapshot;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    pub extracted: usize,
    pub duplicates: usize,
    pub versions: Vec<(ItemId, String)>,
}

impl Snapshot {
    pub fn hints(&self, id: ItemId) -> Hints {
        let own: Vec<&ItemFile> = self.catalog.files.iter().filter(|file| file.item_id == id).collect();
        let fingerprint = |blob: BlobId| {
            self.catalog
                .blobs
                .iter()
                .find(|candidate| candidate.id == blob)
                .and_then(|blob| blob.fingerprint.as_deref())
        };
        let mine: HashSet<&str> = own.iter().filter_map(|file| fingerprint(file.blob_id)).collect();
        let duplicates = self
            .catalog
            .files
            .iter()
            .filter(|file| !own.iter().any(|mine| mine.blob_id == file.blob_id))
            .filter(|file| fingerprint(file.blob_id).is_some_and(|print| mine.contains(print)))
            .count();
        let title = self.item(id).map(|item| item.title.to_lowercase());
        let version = self.facts.get(&id).and_then(|facts| facts.attribute(FacetKey::Version));
        let mut versions: Vec<(ItemId, String)> = self
            .catalog
            .items
            .iter()
            .filter(|item| item.id != id && Some(item.title.to_lowercase()) == title)
            .filter_map(|item| {
                let other = self.facts.get(&item.id)?.attribute(FacetKey::Version)?;
                (Some(other) != version).then(|| (item.id, other.to_owned()))
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
