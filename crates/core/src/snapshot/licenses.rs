use std::collections::HashMap;
use std::sync::Arc;

use snowcase_store::{BlobId, ItemId, PackageKey};

use super::Snapshot;
use crate::ingest::vn3::Vn3License;

pub(crate) type Licenses = HashMap<String, Vec<(String, Option<Arc<Vn3License>>)>>;

pub(crate) fn record(licenses: &mut Licenses, key: PackageKey, license: Option<Arc<Vn3License>>) {
    let rows = licenses.entry(key.fingerprint).or_default();
    rows.retain(|(member, _)| *member != key.member);
    rows.push((key.member, license));
}

impl Snapshot {
    pub(crate) fn license_scanned(&self, blob: BlobId, member: &str) -> bool {
        self.fingerprint(blob)
            .and_then(|fingerprint| self.licenses.get(fingerprint))
            .is_some_and(|rows| rows.iter().any(|(scanned, _)| scanned == member))
    }

    pub fn licenses_of(&self, id: ItemId) -> Vec<&Vn3License> {
        let mut found: Vec<&Vn3License> = Vec::new();
        for file in self.item_files(id) {
            let rows = self.fingerprint(file.blob_id).and_then(|fingerprint| self.licenses.get(fingerprint));
            for license in rows.into_iter().flatten().filter_map(|(_, license)| license.as_deref()) {
                if !found.contains(&license) {
                    found.push(license);
                }
            }
        }
        found
    }
}
