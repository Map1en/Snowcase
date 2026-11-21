use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use snowcase_store::{Cache, ItemId, Store, StoreError};

use crate::ingest::scan::ArchiveCache;
use crate::snapshot::{PackageSummary, Snapshot};

mod edit;
mod jobs;
mod scanning;

pub use edit::{Picture, StorePage};
pub use jobs::{Done, Outcome, Task, Work, run};
pub use scanning::ScanReport;

const MEDIA: &str = "media";

pub const DEFAULT_FOLDER: &str = "Default";

pub struct Library {
    store: Store,
    cache: Cache,
    snapshot: Arc<Snapshot>,
    archives: ArchiveCache,
}

impl Library {
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let mut store = Store::open(dir)?;
        let mut catalog = store.load()?;
        if catalog.folders.is_empty() {
            store.create_folder(None, DEFAULT_FOLDER)?;
            catalog = store.load()?;
        }
        let mut cache = Cache::open(dir)?;
        let mut snapshot = Snapshot::new(catalog);
        snapshot.media_dir = dir.join(MEDIA);
        snapshot.packages = Arc::new(
            cache
                .load_packages()?
                .iter()
                .map(|scanned| (scanned.key.clone(), Arc::new(PackageSummary::of(scanned))))
                .collect(),
        );
        Ok(Self {
            store,
            cache,
            snapshot: Arc::new({
                snapshot.reindex();
                snapshot
            }),
            archives: ArchiveCache::default(),
        })
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.clone()
    }

    fn refresh_items(&mut self, ids: &[ItemId]) -> Result<(), StoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self.store.load_items(ids)?;
        Arc::make_mut(&mut self.snapshot).apply_items(ids, rows);
        Ok(())
    }
}

impl Deref for Library {
    type Target = Snapshot;

    fn deref(&self) -> &Snapshot {
        &self.snapshot
    }
}

#[cfg(test)]
mod tests;
