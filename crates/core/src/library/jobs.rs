use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::sync::Arc;

use snowcase_formats::{
    FileKind, PackageAsset, QUICK_SPAN, crc32, is_junk, list_unitypackage, list_zipped_unitypackage, pdf_text,
    quick_fingerprint, read_zip_member,
};
use snowcase_store::{
    BlobId, FacetKey, FacetOrigin, FacetValue, ItemId, LocationState, PackageKey, ScannedLicense, ScannedPackage,
    StoreError,
};

use super::Library;
use crate::ingest::vn3::{PARSER_VERSION, Vn3License, parse_vn3};
use crate::snapshot::{PackageSummary, item_node, record};

const LICENSE_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    Fingerprint,
    Crc { expected: u32 },
    Package { member: Option<String>, bytes: u64, fingerprint: String },
    License { member: Option<String>, bytes: u64, fingerprint: String },
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
            Task::Package { bytes, .. } | Task::License { bytes, .. } => bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    Fingerprint(String),
    Crc(u32),
    Package(Result<Vec<PackageAsset>, String>),
    License(Result<Option<Vn3License>, String>),
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
                let reader = BufReader::new(file);
                let assets = match member {
                    None => list_unitypackage(reader),
                    Some(member) => list_zipped_unitypackage(reader, member),
                };
                Ok(Done::Package(assets.map_err(|error| error.to_string())))
            }
            Task::License { member, .. } => {
                let mut reader = BufReader::new(file);
                let bytes = match member {
                    None => {
                        let mut bytes = Vec::new();
                        reader.read_to_end(&mut bytes).map(|_| bytes)
                    }
                    Some(member) => read_zip_member(reader, member, LICENSE_LIMIT),
                };
                let text = bytes.map_err(|error| error.to_string()).and_then(|bytes| pdf_text(&bytes));
                Ok(Done::License(text.map(|text| parse_vn3(&text))))
            }
        })
        .map_err(|error| error.to_string());
    Outcome { work, result }
}

impl Library {
    fn present_paths(&self) -> HashMap<BlobId, PathBuf> {
        let mut present: HashMap<BlobId, PathBuf> = HashMap::new();
        for location in &self.snapshot.catalog.locations {
            if location.state == LocationState::Present {
                present
                    .entry(location.blob_id)
                    .or_insert_with(|| PathBuf::from(self.location_path(location)));
            }
        }
        present
    }

    pub fn pending_files(&self) -> Vec<Work> {
        let catalog = &self.snapshot.catalog;
        let present = self.present_paths();
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
        let mut work = Vec::new();
        for blob in &catalog.blobs {
            let Some(path) = present.get(&blob.id) else {
                continue;
            };
            let size = u64::try_from(blob.size).unwrap_or(0);
            if blob.fingerprint.is_none() {
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
        }
        work.sort_by(|a, b| a.path.cmp(&b.path));
        work
    }

    pub fn pending_contents(&self) -> Vec<Work> {
        let catalog = &self.snapshot.catalog;
        let present = self.present_paths();
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
            let (Some(path), Some(fingerprint)) = (present.get(&blob.id), &blob.fingerprint) else {
                continue;
            };
            let size = u64::try_from(blob.size).unwrap_or(0);
            let mut push = |task: Task| {
                work.push(Work {
                    blob_id: blob.id,
                    path: path.clone(),
                    size,
                    task,
                });
            };
            let package = |member: Option<String>, bytes: u64| Task::Package {
                member,
                bytes,
                fingerprint: fingerprint.clone(),
            };
            let license = |member: Option<String>, bytes: u64| Task::License {
                member,
                bytes,
                fingerprint: fingerprint.clone(),
            };
            let is_license = |path: &str, bytes: u64| {
                bytes <= LICENSE_LIMIT && path.to_lowercase().ends_with(".pdf") && !is_junk(path)
            };
            if FileKind::from(blob.kind) == FileKind::UnityPackage {
                let copy_of = catalog
                    .files
                    .iter()
                    .find(|file| file.blob_id == blob.id && file.extracted_from_blob_id.is_some())
                    .and_then(|file| Some((file.extracted_from_blob_id?, file.extracted_from_path.as_deref()?)));
                let known = scanned(blob.id, "") || copy_of.is_some_and(|(archive, member)| scanned(archive, member));
                if !known {
                    push(package(None, size));
                }
            }
            let path_text = path.to_string_lossy();
            if is_license(&path_text, size) && !self.license_scanned(blob.id, "") {
                push(license(None, size));
            }
            if path_text.to_lowercase().ends_with(".zip")
                && let Some(entries) = self.archives.get(&path_text)
            {
                for entry in entries {
                    let is_package = entry.path.to_lowercase().ends_with(".unitypackage") && !is_junk(&entry.path);
                    let covered = unpacked.get(&blob.id).is_some_and(|members| members.contains(entry.path.as_str()));
                    if covered {
                        continue;
                    }
                    if is_package && !scanned(blob.id, &entry.path) {
                        push(package(Some(entry.path.clone()), entry.size));
                    }
                    if is_license(&entry.path, entry.size) && !self.license_scanned(blob.id, &entry.path) {
                        push(license(Some(entry.path.clone()), entry.size));
                    }
                }
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
        let mut licenses = Vec::new();
        self.store.transaction(|store| {
            for outcome in outcomes {
                let blob_id = outcome.work.blob_id;
                match (&outcome.result, &outcome.work.task) {
                    (Ok(Done::Fingerprint(fingerprint)), _) => store.set_fingerprint(blob_id, fingerprint)?,
                    (Ok(Done::Package(assets)), Task::Package { member, fingerprint, .. }) => {
                        packages.push(ScannedPackage {
                            key: PackageKey {
                                fingerprint: fingerprint.clone(),
                                member: member.clone().unwrap_or_default(),
                            },
                            result: assets.clone(),
                        });
                    }
                    (Ok(Done::License(result)), Task::License { member, fingerprint, .. }) => {
                        let key = PackageKey {
                            fingerprint: fingerprint.clone(),
                            member: member.clone().unwrap_or_default(),
                        };
                        licenses.push((key, result.clone()));
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
        let scanned: Vec<ScannedLicense> = licenses
            .iter()
            .map(|(key, result)| ScannedLicense {
                key: key.clone(),
                parser: PARSER_VERSION,
                result: result
                    .as_ref()
                    .map(|license| license.as_ref().and_then(|license| serde_json::to_string(license).ok()))
                    .map_err(Clone::clone),
            })
            .collect();
        self.cache.save_licenses(&scanned)?;
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let known = Arc::make_mut(&mut snapshot.licenses);
        for (key, result) in licenses {
            record(known, key, result.ok().flatten().map(Arc::new));
        }
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
