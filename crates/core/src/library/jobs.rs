use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

use snowcase_formats::{
    FileKind, PackageAsset, QUICK_SPAN, crc32, is_junk, list_unitypackage, list_zipped_unitypackage,
    quick_fingerprint,
};
use snowcase_store::{
    BlobId, FacetKey, FacetOrigin, FacetValue, ItemId, LocationState, PackageKey, ScannedPackage, StoreError,
};

use super::{Library, PackageSummary, item_node};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    Fingerprint,
    Crc { expected: u32 },
    Package { member: Option<String>, bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub blob_id: BlobId,
    pub path: PathBuf,
    pub size: u64,
    pub task: Task,
}

impl Work {
    pub fn bytes(&self) -> u64 {
        match self.task {
            Task::Fingerprint => self.size.min(2 * QUICK_SPAN),
            Task::Crc { .. } => self.size,
            Task::Package { bytes, .. } => bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    Fingerprint(String),
    Crc(u32),
    Package {
        fingerprint: String,
        assets: Result<Vec<PackageAsset>, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub work: Work,
    pub result: Result<Done, String>,
}

pub fn run(work: Work) -> Outcome {
    let result = File::open(&work.path)
        .and_then(|file| match &work.task {
            Task::Fingerprint => quick_fingerprint(file, work.size).map(Done::Fingerprint),
            Task::Crc { .. } => crc32(BufReader::new(file)).map(Done::Crc),
            Task::Package { member, .. } => {
                let fingerprint = quick_fingerprint(file, work.size)?;
                let reader = BufReader::new(File::open(&work.path)?);
                let assets = match member {
                    None => list_unitypackage(reader),
                    Some(member) => list_zipped_unitypackage(reader, member),
                };
                Ok(Done::Package {
                    fingerprint,
                    assets: assets.map_err(|error| error.to_string()),
                })
            }
        })
        .map_err(|error| error.to_string());
    Outcome { work, result }
}

impl Library {
    pub fn pending_work(&self) -> Vec<Work> {
        let catalog = &self.snapshot.catalog;
        let mut present: HashMap<BlobId, PathBuf> = HashMap::new();
        for location in &catalog.locations {
            if location.state == LocationState::Present {
                present
                    .entry(location.blob_id)
                    .or_insert_with(|| PathBuf::from(self.location_path(location)));
            }
        }
        let mut expected: HashMap<BlobId, u32> = HashMap::new();
        for file in &catalog.files {
            let (Some(archive), Some(member)) = (file.extracted_from_blob_id, &file.extracted_from_path) else {
                continue;
            };
            let crc = present
                .get(&archive)
                .and_then(|path| self.archives.get(&path.to_string_lossy()))
                .and_then(|entries| entries.iter().find(|entry| &entry.path == member))
                .map(|entry| entry.crc32);
            if let Some(crc) = crc {
                expected.insert(file.blob_id, crc);
            }
        }
        let scanned = |blob: BlobId, member: &str| {
            self.package_key(blob, member)
                .is_some_and(|key| self.packages.contains_key(&key))
        };
        let mut unpacked: HashMap<BlobId, HashSet<&str>> = HashMap::new();
        for file in &catalog.files {
            if let (Some(archive), Some(member)) = (file.extracted_from_blob_id, &file.extracted_from_path) {
                unpacked.entry(archive).or_default().insert(member);
            }
        }
        let mut work = Vec::new();
        for blob in &catalog.blobs {
            let Some(path) = present.get(&blob.id) else {
                continue;
            };
            let size = u64::try_from(blob.size).unwrap_or(0);
            let mut packages = Vec::new();
            if FileKind::from(blob.kind) == FileKind::UnityPackage {
                let copy_of = catalog
                    .files
                    .iter()
                    .find(|file| file.blob_id == blob.id && file.extracted_from_blob_id.is_some())
                    .and_then(|file| Some((file.extracted_from_blob_id?, file.extracted_from_path.as_deref()?)));
                let known = scanned(blob.id, "") || copy_of.is_some_and(|(archive, member)| scanned(archive, member));
                if !known {
                    packages.push(Task::Package { member: None, bytes: size });
                }
            }
            let path_text = path.to_string_lossy();
            if path_text.to_lowercase().ends_with(".zip")
                && let Some(entries) = self.archives.get(&path_text)
            {
                for entry in entries {
                    let is_package = entry.path.to_lowercase().ends_with(".unitypackage") && !is_junk(&entry.path);
                    let covered = unpacked.get(&blob.id).is_some_and(|members| members.contains(entry.path.as_str()));
                    if is_package && !covered && !scanned(blob.id, &entry.path) {
                        packages.push(Task::Package {
                            member: Some(entry.path.clone()),
                            bytes: entry.size,
                        });
                    }
                }
            }
            if blob.fingerprint.is_none() && packages.is_empty() {
                work.push(Work {
                    blob_id: blob.id,
                    path: path.clone(),
                    size,
                    task: Task::Fingerprint,
                });
            }
            if blob.crc32.is_none()
                && let Some(&crc) = expected.get(&blob.id)
            {
                work.push(Work {
                    blob_id: blob.id,
                    path: path.clone(),
                    size,
                    task: Task::Crc { expected: crc },
                });
            }
            for task in packages {
                work.push(Work {
                    blob_id: blob.id,
                    path: path.clone(),
                    size,
                    task,
                });
            }
        }
        work.sort_by(|a, b| a.path.cmp(&b.path));
        work
    }

    pub fn apply_work(&mut self, outcomes: &[Outcome]) -> Result<(), StoreError> {
        let catalog = &self.snapshot.catalog;
        let mut owners: HashMap<BlobId, Vec<(ItemId, bool)>> = HashMap::new();
        for file in &catalog.files {
            owners
                .entry(file.blob_id)
                .or_default()
                .push((file.item_id, file.extracted_from_blob_id.is_some()));
        }
        let mut touched = HashSet::new();
        let mut packages = Vec::new();
        self.store.transaction(|store| {
            for outcome in outcomes {
                let blob_id = outcome.work.blob_id;
                match (&outcome.result, &outcome.work.task) {
                    (Ok(Done::Fingerprint(fingerprint)), _) => store.set_fingerprint(blob_id, fingerprint)?,
                    (Ok(Done::Package { fingerprint, assets }), Task::Package { member, .. }) => {
                        store.set_fingerprint(blob_id, fingerprint)?;
                        packages.push(ScannedPackage {
                            key: PackageKey {
                                fingerprint: fingerprint.clone(),
                                member: member.clone().unwrap_or_default(),
                            },
                            result: assets.clone(),
                        });
                    }
                    (Ok(Done::Crc(crc)), Task::Crc { expected }) => {
                        store.set_crc32(blob_id, *crc)?;
                        if crc != expected {
                            for (item, extracted) in owners.get(&blob_id).into_iter().flatten() {
                                if *extracted {
                                    store.set_extracted_from(*item, blob_id, None)?;
                                }
                            }
                        }
                    }
                    _ => continue,
                }
                for (item, _) in owners.get(&blob_id).into_iter().flatten() {
                    touched.insert(*item);
                }
            }
            Ok(())
        })?;
        self.cache.save_packages(&packages)?;
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let summaries = Arc::make_mut(&mut snapshot.packages);
        for scanned in &packages {
            summaries.insert(scanned.key.clone(), Arc::new(PackageSummary::of(scanned)));
        }
        let mut touched: Vec<ItemId> = touched.into_iter().collect();
        touched.sort();
        self.refresh_items(&touched)?;
        if packages.is_empty() {
            return Ok(());
        }
        let shops: Vec<(ItemId, Vec<FacetValue>)> = touched
            .iter()
            .map(|id| {
                let shop = self.shop_from_packages(*id).map(|value| FacetValue {
                    value,
                    target_item_id: None,
                });
                (*id, shop.into_iter().collect())
            })
            .collect();
        self.store.transaction(|store| {
            for (id, shop) in &shops {
                store.replace_facets(&item_node(*id), FacetKey::Shop, FacetOrigin::Auto, shop)?;
            }
            Ok(())
        })?;
        self.refresh_items(&touched)
    }
}

#[cfg(test)]
mod tests;
