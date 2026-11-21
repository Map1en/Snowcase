use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use snowcase_store::{BlobId, FacetKey, ItemFile, ItemId, PackageKey, ScannedPackage};

use super::Snapshot;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageSummary {
    pub files: usize,
    pub root: Option<String>,
    pub shop: Option<String>,
    pub error: Option<String>,
    pub paths: Vec<String>,
}

const SHOP_SHARE: f64 = 0.6;

pub(crate) fn shop_of(files: &[&str]) -> Option<String> {
    let mut by_top: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for path in files {
        let parts: Vec<&str> = path.split('/').collect();
        if let ["Assets", top, product, _, ..] = parts.as_slice() {
            *by_top.entry(top).or_default().entry(product).or_default() += 1;
        }
    }
    let (top, products) = by_top
        .into_iter()
        .max_by(|a, b| a.1.values().sum::<usize>().cmp(&b.1.values().sum()).then_with(|| b.0.cmp(a.0)))?;
    let under_top: usize = products.values().sum();
    let largest = products.values().copied().max().unwrap_or(0);
    let dominant = under_top as f64 >= files.len() as f64 * SHOP_SHARE;
    let one_product = largest as f64 >= under_top as f64 * SHOP_SHARE;
    (dominant && one_product).then(|| top.to_owned())
}

impl PackageSummary {
    pub(crate) fn of(scanned: &ScannedPackage) -> Self {
        match &scanned.result {
            Err(error) => Self {
                error: Some(error.clone()),
                ..Self::default()
            },
            Ok(assets) => {
                let files: Vec<&str> = assets.iter().filter(|asset| asset.is_file).map(|asset| asset.path.as_str()).collect();
                let mut roots: HashMap<String, usize> = HashMap::new();
                for path in &files {
                    let depth = if path.starts_with("Packages/") { 2 } else { 3 };
                    let parts: Vec<&str> = path.split('/').collect();
                    if parts.len() > depth {
                        *roots.entry(parts[..depth].join("/")).or_default() += 1;
                    }
                }
                Self {
                    files: files.len(),
                    root: roots
                        .into_iter()
                        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                        .map(|(root, _)| root),
                    shop: shop_of(&files),
                    error: None,
                    paths: files.iter().map(|path| (*path).to_owned()).collect(),
                }
            }
        }
    }
}

impl Snapshot {
    pub(crate) fn fingerprint(&self, blob: BlobId) -> Option<&str> {
        self.catalog
            .blobs
            .iter()
            .find(|candidate| candidate.id == blob)
            .and_then(|blob| blob.fingerprint.as_deref())
    }

    pub(crate) fn package_key(&self, blob: BlobId, member: &str) -> Option<PackageKey> {
        Some(PackageKey {
            fingerprint: self.fingerprint(blob)?.to_owned(),
            member: member.to_owned(),
        })
    }

    pub fn package_of(&self, file: &ItemFile) -> Option<&PackageSummary> {
        let own = self.package_key(file.blob_id, "").and_then(|key| self.packages.get(&key));
        own.or_else(|| {
            let (archive, member) = (file.extracted_from_blob_id?, file.extracted_from_path.as_deref()?);
            self.packages.get(&self.package_key(archive, member)?)
        })
        .map(Arc::as_ref)
    }

    fn package_roots(&self, id: ItemId) -> HashSet<&str> {
        self.catalog
            .files
            .iter()
            .filter(|file| file.item_id == id)
            .flat_map(|file| {
                let inside = self.packages_in(file.blob_id).into_iter().map(|(_, summary)| summary);
                self.package_of(file).into_iter().chain(inside).collect::<Vec<_>>()
            })
            .filter_map(|summary| summary.root.as_deref())
            .collect()
    }

    pub fn merge_candidates(&self, id: ItemId) -> Vec<ItemId> {
        let roots = self.package_roots(id);
        if roots.is_empty() {
            return Vec::new();
        }
        let version = |item: ItemId| self.facts.get(&item).and_then(|facts| facts.attribute(FacetKey::Version));
        let mine = version(id);
        self.catalog
            .items
            .iter()
            .map(|item| item.id)
            .filter(|other| *other != id)
            .filter(|other| match (mine, version(*other)) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            })
            .filter(|other| !self.package_roots(*other).is_disjoint(&roots))
            .collect()
    }

    pub(crate) fn shop_from_packages(&self, id: ItemId) -> Option<String> {
        let mut shops: HashMap<&str, usize> = HashMap::new();
        for file in self.catalog.files.iter().filter(|file| file.item_id == id) {
            let inside = self.packages_in(file.blob_id).into_iter().map(|(_, summary)| summary);
            for summary in self.package_of(file).into_iter().chain(inside) {
                if let Some(shop) = &summary.shop {
                    *shops.entry(shop).or_default() += summary.files;
                }
            }
        }
        shops
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(shop, _)| shop.to_owned())
    }

    pub fn packages_in(&self, blob: BlobId) -> Vec<(&str, &PackageSummary)> {
        let Some(fingerprint) = self.fingerprint(blob) else {
            return Vec::new();
        };
        let mut found: Vec<(&str, &PackageSummary)> = self
            .packages
            .iter()
            .filter(|(key, _)| key.fingerprint == fingerprint && !key.member.is_empty())
            .map(|(key, summary)| (key.member.as_str(), summary.as_ref()))
            .collect();
        found.sort_by_key(|(member, _)| *member);
        found
    }
}
