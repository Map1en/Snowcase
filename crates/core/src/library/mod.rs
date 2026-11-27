use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use snowcase_store::{Cache, ItemId, Store, StoreError};

use crate::ingest::scan::ArchiveCache;
use crate::ingest::vn3::PARSER_VERSION;
use crate::snapshot::{Licenses, PackageSummary, Snapshot, record};

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
        let opened = Cache::open(dir).and_then(|mut cache| {
            let packages = cache.load_packages()?;
            let licenses = cache.load_licenses()?;
            Ok((cache, packages, licenses))
        });
        let (cache, packages, scanned_licenses) = match opened {
            Ok(opened) => opened,
            Err(_) => {
                Cache::remove(dir)?;
                (Cache::open(dir)?, Vec::new(), Vec::new())
            }
        };
        let mut snapshot = Snapshot::new(catalog);
        snapshot.media_dir = dir.join(MEDIA);
        snapshot.packages = Arc::new(
            packages
                .iter()
                .map(|scanned| (scanned.key.clone(), Arc::new(PackageSummary::of(scanned))))
                .collect(),
        );
        let mut licenses = Licenses::new();
        for scanned in scanned_licenses.into_iter().filter(|scanned| scanned.parser == PARSER_VERSION) {
            let license = match &scanned.result {
                Ok(Some(json)) => match serde_json::from_str(json) {
                    Ok(license) => Some(Arc::new(license)),
                    Err(_) => continue,
                },
                _ => None,
            };
            record(&mut licenses, scanned.key, license);
        }
        snapshot.licenses = Arc::new(licenses);
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
