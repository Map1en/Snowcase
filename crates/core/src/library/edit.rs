use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use snowcase_store::{BlobKind, FacetKey, FacetNode, FacetOrigin, FacetValue, FileImport, FolderId, ImageOrigin, ItemChanges, ItemId, Media, RootId, Store, StoreError, StoreKind, TagId};

use super::Library;
use crate::ingest::files::FileInfo;
use crate::ingest::names::tag_key;
use crate::snapshot::media_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePage {
    pub external_id: String,
    pub name: String,
    pub shop: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub bytes: Vec<u8>,
    pub mime: String,
}

impl Library {
    pub fn import(&mut self, files: Vec<FileInfo>) -> Result<usize, StoreError> {
        let mut known: BTreeSet<String> = self
            .snapshot
            .catalog
            .locations
            .iter()
            .filter(|location| location.root_id.is_none())
            .map(|location| location.rel_path.clone())
            .collect();
        let fresh: Vec<FileInfo> = files
            .into_iter()
            .filter(|file| known.insert(file.path.replace('\\', "/")))
            .collect();
        let added = fresh.len();
        let ids = self.store.transaction(|store| {
            fresh
                .into_iter()
                .map(|file| import_file(store, file))
                .collect::<Result<Vec<_>, _>>()
        })?;
        self.refresh_items(&ids)?;
        Ok(added)
    }

    pub fn update_item(&mut self, id: ItemId, changes: &ItemChanges) -> Result<(), StoreError> {
        self.store.update_item(id, changes)?;
        self.refresh_items(&[id])
    }

    pub fn tag(&mut self, node: &FacetNode, name: &str) -> Result<Option<TagId>, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let tag = self.store.tag_node(node, name, &tag_key(name))?;
        let tags = self.store.load_tags()?;
        Arc::make_mut(&mut self.snapshot).catalog.tags = tags;
        self.refresh_items(&[node.item_id])?;
        Ok(Some(tag))
    }

    pub fn untag(&mut self, node: &FacetNode, tag: TagId) -> Result<(), StoreError> {
        self.store.untag_node(node, tag)?;
        self.refresh_items(&[node.item_id])
    }

    pub fn create_folder(
        &mut self,
        parent: Option<FolderId>,
        name: &str,
    ) -> Result<Option<FolderId>, StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(existing) = self.snapshot.catalog.folders.iter().find(|folder| {
            folder.parent_id == parent && folder.name.to_lowercase() == name.to_lowercase()
        }) {
            return Ok(Some(existing.id));
        }
        let folder = self.store.create_folder(parent, name)?;
        let folders = self.store.load_folders()?;
        Arc::make_mut(&mut self.snapshot).catalog.folders = folders;
        Ok(Some(folder))
    }

    pub fn put_in_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.put_in_folder(id, folder)?;
        self.refresh_items(&[id])
    }

    pub fn remove_from_folder(&mut self, id: ItemId, folder: FolderId) -> Result<(), StoreError> {
        self.store.remove_from_folder(id, folder)?;
        self.refresh_items(&[id])
    }

    pub fn link_booth(&mut self, id: ItemId, page: &StorePage, picture: Option<&Picture>) -> Result<(), StoreError> {
        let media = match picture {
            Some(picture) => {
                let hash = blake3::hash(&picture.bytes).to_hex().to_string();
                let path = media_path(&self.snapshot.media_dir, &hash, &picture.mime);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if !path.exists() {
                    std::fs::write(&path, &picture.bytes)?;
                }
                Some(Media {
                    hash,
                    mime: picture.mime.clone(),
                    width: None,
                    height: None,
                    bytes: i64::try_from(picture.bytes.len()).unwrap_or(i64::MAX),
                })
            }
            None => None,
        };
        self.store.transaction(|store| {
            store.update_item(
                id,
                &ItemChanges {
                    title: Some(page.name.clone()),
                    note: None,
                },
            )?;
            store.set_store_source(id, StoreKind::Booth, &page.url, &page.external_id, &page.shop)?;
            if let Some(media) = &media {
                store.set_cover(id, media, ImageOrigin::Store)?;
            }
            Ok(())
        })?;
        let media = self.store.load_media()?;
        Arc::make_mut(&mut self.snapshot).catalog.media = media;
        self.refresh_items(&[id])
    }

    pub fn remove_root(&mut self, root: RootId) -> Result<(), StoreError> {
        let affected = self.items_in_root(root);
        self.store.remove_root(root)?;
        let roots = self.store.load_roots()?;
        Arc::make_mut(&mut self.snapshot).catalog.roots = roots;
        self.refresh_items(&affected)
    }

    pub fn merge_items(&mut self, into: ItemId, from: ItemId) -> Result<(), StoreError> {
        if into == from {
            return Ok(());
        }
        self.store.merge_items(into, from)?;
        self.refresh_items(&[into, from])
    }

    pub fn set_auto_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        self.write_facets(node, key, FacetOrigin::Auto, values)
    }

    pub fn set_user_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        self.write_facets(node, key, FacetOrigin::User, values)
    }

    pub fn remove_facet(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        value: &str,
    ) -> Result<(), StoreError> {
        self.store.remove_facet(node, key, value)?;
        self.refresh_items(&[node.item_id])
    }

    fn write_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        origin: FacetOrigin,
        values: &[FacetValue],
    ) -> Result<(), StoreError> {
        assert!(
            key.multi_valued() || values.len() <= 1,
            "{key} takes a single value, got {}",
            values.len()
        );
        self.store.replace_facets(node, key, origin, values)?;
        self.refresh_items(&[node.item_id])
    }
}

fn import_file(store: &mut Store, file: FileInfo) -> Result<ItemId, StoreError> {
    let item = store.create_item(&title_of(&file.name))?;
    store.add_file(&FileImport {
        item_id: item,
        root_id: None,
        rel_path: file.path.replace('\\', "/"),
        file_name: file.name,
        display_name: None,
        size: i64::try_from(file.size).unwrap_or(i64::MAX),
        modified_ms: file.modified_ms,
        crc32: None,
        kind: BlobKind::from(file.kind),
    })?;
    Ok(item)
}

fn title_of(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| file_name.to_owned())
}
