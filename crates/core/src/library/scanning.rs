use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use snowcase_store::{
    BlobId, BlobKind, FacetKey, FacetNode, FacetOrigin, FacetValue, FileImport, ItemId, LocationId,
    LocationState, StoreError,
};

use super::Library;
use crate::ingest::names::{CodePage, version_of};
use crate::ingest::scan::{self, ArchiveCache, FoundFile, KnownFile, RootScan, WalkError};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub new_items: usize,
    pub new_files: usize,
    pub moved: usize,
    pub missing: usize,
    pub removed_copies: usize,
    pub unity_projects: Vec<String>,
    pub unsupported: Vec<String>,
    pub errors: Vec<WalkError>,
}

struct Known {
    id: LocationId,
    blob_id: BlobId,
    rel_path: String,
    size: u64,
    modified_ms: i64,
    present: bool,
}

impl ScanReport {
    pub fn absorb(&mut self, root: &Path, other: ScanReport) {
        let full = |rel: String| root.join(rel).to_string_lossy().into_owned();
        self.new_items += other.new_items;
        self.new_files += other.new_files;
        self.moved += other.moved;
        self.missing += other.missing;
        self.removed_copies += other.removed_copies;
        self.unity_projects.extend(other.unity_projects.into_iter().map(full));
        self.unsupported.extend(other.unsupported.into_iter().map(full));
        self.errors.extend(other.errors);
    }
}

impl Library {
    pub fn scan_roots(&mut self, code_page: CodePage) -> Result<ScanReport, StoreError> {
        let roots: Vec<String> = self.snapshot.catalog.roots.iter().map(|root| root.path.clone()).collect();
        let mut report = ScanReport::default();
        for root in roots {
            let root = Path::new(&root);
            if root.is_dir() {
                let scanned = self.scan_root(root, code_page)?;
                report.absorb(root, scanned);
            }
        }
        Ok(report)
    }

    pub fn scan_root(&mut self, root: &Path, code_page: CodePage) -> Result<ScanReport, StoreError> {
        let scan = scan::read_root(root, code_page, &self.archives);
        self.apply_scan(scan)
    }

    pub fn archive_cache(&self) -> ArchiveCache {
        self.archives.clone()
    }

    pub fn apply_scan(&mut self, scan: RootScan) -> Result<ScanReport, StoreError> {
        let RootScan { root, walk, plan, fresh } = scan;
        self.archives.absorb(fresh);
        let root_text = root
            .to_str()
            .ok_or_else(|| StoreError::NonUtf8Path(root.clone()))?
            .to_owned();

        let catalog = &self.snapshot.catalog;
        let existing_root = catalog.roots.iter().find(|r| r.path == root_text).map(|r| r.id);
        let sizes: HashMap<BlobId, u64> = catalog
            .blobs
            .iter()
            .map(|blob| (blob.id, u64::try_from(blob.size).unwrap_or(0)))
            .collect();
        let known: Vec<Known> = catalog
            .locations
            .iter()
            .filter(|location| existing_root.is_some() && location.root_id == existing_root)
            .map(|location| Known {
                id: location.id,
                blob_id: location.blob_id,
                rel_path: location.rel_path.clone(),
                size: sizes.get(&location.blob_id).copied().unwrap_or(0),
                modified_ms: location.modified_ms,
                present: location.state == LocationState::Present,
            })
            .collect();
        let mut owners: HashMap<BlobId, Vec<(ItemId, Option<BlobId>)>> = HashMap::new();
        for file in &catalog.files {
            owners
                .entry(file.blob_id)
                .or_default()
                .push((file.item_id, file.extracted_from_blob_id));
        }
        let mut locations_of: HashMap<BlobId, usize> = HashMap::new();
        for location in &catalog.locations {
            *locations_of.entry(location.blob_id).or_default() += 1;
        }

        let mut touched: HashSet<ItemId> = HashSet::new();
        let report = self.store.transaction(|store| {
            let root_id = store.add_root(&root_text)?;
            let mut report = ScanReport {
                unity_projects: walk.unity_projects.clone(),
                unsupported: plan.unsupported.clone(),
                errors: walk.errors.clone(),
                ..ScanReport::default()
            };
            let known_files: Vec<KnownFile> = known
                .iter()
                .map(|k| KnownFile {
                    rel_path: k.rel_path.clone(),
                    size: k.size,
                    modified_ms: k.modified_ms,
                })
                .collect();
            let changes = scan::diff(&known_files, &walk.files);
            let by_path: HashMap<&str, &Known> = known.iter().map(|k| (k.rel_path.as_str(), k)).collect();
            let found: HashMap<&str, &FoundFile> = walk.files.iter().map(|f| (f.rel_path.as_str(), f)).collect();
            let mut path_blob: HashMap<String, BlobId> = HashMap::new();

            let mut arrivals: BTreeMap<(u64, i64), Vec<&FoundFile>> = BTreeMap::new();
            for file in &changes.new {
                arrivals.entry((file.size, file.modified_ms)).or_default().push(file);
            }
            let mut gone = Vec::new();
            for path in &changes.missing {
                let k = by_path[path.as_str()];
                match arrivals.get_mut(&(k.size, k.modified_ms)).and_then(|files| (!files.is_empty()).then(|| files.remove(0))) {
                    Some(target) => {
                        store.move_location(k.id, &target.rel_path, target.modified_ms)?;
                        path_blob.insert(target.rel_path.clone(), k.blob_id);
                        report.moved += 1;
                        touch(&owners, k.blob_id, &mut touched);
                    }
                    None => gone.push(k),
                }
            }
            for file in &walk.files {
                if let Some(k) = by_path.get(file.rel_path.as_str()) {
                    path_blob.insert(file.rel_path.clone(), k.blob_id);
                    if !k.present && k.size == file.size && k.modified_ms == file.modified_ms {
                        store.move_location(k.id, &file.rel_path, file.modified_ms)?;
                        touch(&owners, k.blob_id, &mut touched);
                    }
                }
            }
            let to_add: HashSet<&str> = arrivals
                .values()
                .flatten()
                .map(|file| file.rel_path.as_str())
                .chain(changes.changed.iter().map(|file| file.rel_path.as_str()))
                .collect();

            let mut added_to: HashMap<BlobId, ItemId> = HashMap::new();
            let mut pending = Vec::new();
            for group in &plan.groups {
                let new_files: Vec<_> = group.files.iter().filter(|f| to_add.contains(f.rel_path.as_str())).collect();
                if new_files.is_empty() {
                    continue;
                }
                let mut votes: BTreeMap<ItemId, usize> = BTreeMap::new();
                for file in &group.files {
                    if let Some(blob) = path_blob.get(&file.rel_path) {
                        for (item, _) in owners.get(blob).into_iter().flatten() {
                            *votes.entry(*item).or_default() += 1;
                        }
                    }
                }
                let item = match votes.iter().max_by_key(|(item, count)| (**count, std::cmp::Reverse(**item))) {
                    Some((item, _)) => *item,
                    None => {
                        report.new_items += 1;
                        let item = store.create_item(&group.title)?;
                        if let Some(version) = &group.version {
                            store.replace_facets(
                                &crate::snapshot::item_node(item),
                                FacetKey::Version,
                                FacetOrigin::Auto,
                                &[FacetValue {
                                    value: version.clone(),
                                    target_item_id: None,
                                }],
                            )?;
                        }
                        item
                    }
                };
                touched.insert(item);
                for planned in new_files {
                    let file = found[planned.rel_path.as_str()];
                    let file_name = planned.rel_path.rsplit('/').next().unwrap_or(&planned.rel_path).to_owned();
                    let blob_id = store.add_file(&FileImport {
                        item_id: item,
                        root_id: Some(root_id),
                        rel_path: planned.rel_path.clone(),
                        file_name: file_name.clone(),
                        display_name: planned.display_name.clone(),
                        size: i64::try_from(file.size).unwrap_or(i64::MAX),
                        modified_ms: file.modified_ms,
                        crc32: None,
                        kind: BlobKind::from(file.kind),
                    })?;
                    report.new_files += 1;
                    path_blob.insert(planned.rel_path.clone(), blob_id);
                    added_to.insert(blob_id, item);
                    if let Some(version) = version_of(planned.display_name.as_deref().unwrap_or(&file_name)) {
                        let node = FacetNode {
                            item_id: item,
                            blob_id: Some(blob_id),
                            inner_path: None,
                        };
                        store.replace_facets(
                            &node,
                            FacetKey::Version,
                            FacetOrigin::Auto,
                            &[FacetValue {
                                value: version,
                                target_item_id: None,
                            }],
                        )?;
                    }
                    if let Some((archive, member)) = &planned.extracted_from {
                        pending.push((item, blob_id, archive.clone(), member.clone()));
                    }
                }
            }
            for (item, blob_id, archive, member) in pending {
                let Some(&archive_blob) = path_blob.get(&archive) else {
                    continue;
                };
                let same_item = added_to.get(&archive_blob) == Some(&item)
                    || owners.get(&archive_blob).is_some_and(|o| o.iter().any(|(i, _)| *i == item));
                if same_item {
                    store.set_extracted_from(item, blob_id, Some((archive_blob, &member)))?;
                }
            }

            let mut lost_archives = HashSet::new();
            for k in gone {
                let files = owners.get(&k.blob_id).map(Vec::as_slice).unwrap_or_default();
                let elsewhere = locations_of.get(&k.blob_id).copied().unwrap_or(0) > 1;
                if !elsewhere && !files.is_empty() && files.iter().all(|(_, from)| from.is_some()) {
                    for (item, _) in files {
                        store.remove_item_file(*item, k.blob_id)?;
                        touched.insert(*item);
                    }
                    report.removed_copies += 1;
                } else {
                    store.mark_missing(k.id)?;
                    report.missing += 1;
                    touch(&owners, k.blob_id, &mut touched);
                    if !elsewhere {
                        lost_archives.insert(k.blob_id);
                    }
                }
            }
            for (blob_id, files) in &owners {
                for (item, from) in files {
                    if from.is_some_and(|archive| lost_archives.contains(&archive)) && !lost_archives.contains(blob_id) {
                        store.set_extracted_from(*item, *blob_id, None)?;
                        touched.insert(*item);
                    }
                }
            }
            Ok(report)
        })?;

        let roots = self.store.load_roots()?;
        Arc::make_mut(&mut self.snapshot).catalog.roots = roots;
        let touched: Vec<ItemId> = touched.into_iter().collect();
        self.refresh_items(&touched)?;
        Ok(report)
    }
}

fn touch(owners: &HashMap<BlobId, Vec<(ItemId, Option<BlobId>)>>, blob_id: BlobId, touched: &mut HashSet<ItemId>) {
    for (item, _) in owners.get(&blob_id).into_iter().flatten() {
        touched.insert(*item);
    }
}

#[cfg(test)]
mod tests;
