use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use snowcase_formats::{QUICK_SPAN, crc32, quick_fingerprint};
use snowcase_store::{BlobId, ItemId, LocationState, StoreError};

use super::Library;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Fingerprint,
    Crc { expected: u32 },
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    Fingerprint(String),
    Crc(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub work: Work,
    pub result: Result<Done, String>,
}

pub fn run(work: Work) -> Outcome {
    let result = File::open(&work.path)
        .and_then(|file| match work.task {
            Task::Fingerprint => quick_fingerprint(file, work.size).map(Done::Fingerprint),
            Task::Crc { .. } => crc32(BufReader::new(file)).map(Done::Crc),
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
        let mut work = Vec::new();
        for blob in &catalog.blobs {
            let Some(path) = present.get(&blob.id) else {
                continue;
            };
            let size = u64::try_from(blob.size).unwrap_or(0);
            if !blob.fingerprint.starts_with("b3:") {
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
        self.store.transaction(|store| {
            for outcome in outcomes {
                let blob_id = outcome.work.blob_id;
                match (&outcome.result, outcome.work.task) {
                    (Ok(Done::Fingerprint(fingerprint)), _) => store.set_fingerprint(blob_id, fingerprint)?,
                    (Ok(Done::Crc(crc)), Task::Crc { expected }) => {
                        store.set_crc32(blob_id, *crc)?;
                        if *crc != expected {
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
        let touched: Vec<ItemId> = touched.into_iter().collect();
        self.refresh_items(&touched)
    }
}

#[cfg(test)]
mod tests;
