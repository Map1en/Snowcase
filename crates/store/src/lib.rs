mod cache;
mod model;
mod schema;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use diesel::connection::{AnsiTransactionManager, SimpleConnection, TransactionManager};
use diesel::dsl::max;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::{Sqlite, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub use cache::{Cache, PackageKey, ScannedPackage};
pub use model::*;
use schema::*;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
const DATABASE: &str = "library.db";
const BACKUPS: &str = "backups";

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("failed to prepare the library directory: {0}")]
    Io(#[from] io::Error),
    #[error("failed to open the library database: {0}")]
    Open(#[from] ConnectionError),
    #[error("library database error: {0}")]
    Query(#[from] diesel::result::Error),
    #[error("failed to migrate the library database: {0}")]
    Migration(BoxError),
    #[error("the library was written by a newer Snowcase (unknown migrations: {})", .0.join(", "))]
    NewerLibrary(Vec<String>),
    #[error("the library path is not valid UTF-8: {0}")]
    NonUtf8Path(PathBuf),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

pub struct Store {
    dir: PathBuf,
    conn: SqliteConnection,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let database = dir.join(DATABASE);
        let url = database
            .to_str()
            .ok_or_else(|| StoreError::NonUtf8Path(database.clone()))?;
        let mut conn = SqliteConnection::establish(url)?;
        conn.batch_execute(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let mut store = Self {
            dir: dir.to_path_buf(),
            conn,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let known: HashSet<String> = diesel::migration::MigrationSource::<Sqlite>::migrations(&MIGRATIONS)
            .map_err(StoreError::Migration)?
            .iter()
            .map(|migration| migration.name().version().to_string())
            .collect();
        let applied = self
            .conn
            .applied_migrations()
            .map_err(StoreError::Migration)?;
        let unknown: Vec<String> = applied
            .iter()
            .map(ToString::to_string)
            .filter(|version| !known.contains(version))
            .collect();
        if !unknown.is_empty() {
            return Err(StoreError::NewerLibrary(unknown));
        }
        let pending = self
            .conn
            .has_pending_migration(MIGRATIONS)
            .map_err(StoreError::Migration)?;
        if pending && !applied.is_empty() {
            self.backup()?;
        }
        self.conn
            .run_pending_migrations(MIGRATIONS)
            .map_err(StoreError::Migration)?;
        Ok(())
    }

    fn backup(&mut self) -> Result<PathBuf> {
        let dir = self.dir.join(BACKUPS);
        fs::create_dir_all(&dir)?;
        let mut stamp = now_ms();
        let mut path = dir.join(format!("library.{stamp}.bak"));
        while path.exists() {
            stamp += 1;
            path = dir.join(format!("library.{stamp}.bak"));
        }
        let target = path
            .to_str()
            .ok_or_else(|| StoreError::NonUtf8Path(path.clone()))?;
        diesel::sql_query("VACUUM INTO ?")
            .bind::<Text, _>(target)
            .execute(&mut self.conn)?;
        Ok(path)
    }

    pub fn transaction<T>(&mut self, change: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        AnsiTransactionManager::begin_transaction_sql(&mut self.conn, "BEGIN IMMEDIATE")?;
        match change(self) {
            Ok(value) => {
                AnsiTransactionManager::commit_transaction(&mut self.conn)?;
                Ok(value)
            }
            Err(error) => {
                AnsiTransactionManager::rollback_transaction(&mut self.conn)?;
                Err(error)
            }
        }
    }

    pub fn load(&mut self) -> Result<Catalog> {
        let conn = &mut self.conn;
        let mut catalog = Catalog {
            items: item::table.order(item::id).select(Item::as_select()).load(conn)?,
            sources: item_source::table
                .order(item_source::id)
                .select(ItemSource::as_select())
                .load(conn)?,
            roots: Vec::new(),
            files: item_file::table
                .order((item_file::item_id, item_file::blob_id))
                .select(ItemFile::as_select())
                .load(conn)?,
            facets: facet::table
                .order(facet::id)
                .select(Facet::as_select())
                .load(conn)?,
            blobs: blob::table.order(blob::id).select(Blob::as_select()).load(conn)?,
            locations: blob_location::table
                .order(blob_location::id)
                .select(BlobLocation::as_select())
                .load(conn)?,
            media: media::table
                .order(media::hash)
                .select(Media::as_select())
                .load(conn)?,
            images: item_image::table
                .order((item_image::item_id, item_image::position))
                .select(ItemImage::as_select())
                .load(conn)?,
            tags: Vec::new(),
            node_tags: node_tag::table
                .order(node_tag::id)
                .select(NodeTag::as_select())
                .load(conn)?,
            folders: Vec::new(),
            item_folders: item_folder::table
                .order((item_folder::item_id, item_folder::folder_id))
                .select(ItemFolder::as_select())
                .load(conn)?,
        };
        catalog.roots = self.load_roots()?;
        catalog.tags = self.load_tags()?;
        catalog.folders = self.load_folders()?;
        Ok(catalog)
    }

    pub fn load_roots(&mut self) -> Result<Vec<Root>> {
        Ok(root::table
            .order(root::id)
            .select(Root::as_select())
            .load(&mut self.conn)?)
    }

    pub fn load_tags(&mut self) -> Result<Vec<Tag>> {
        Ok(tag::table
            .order(tag::id)
            .select(Tag::as_select())
            .load(&mut self.conn)?)
    }

    pub fn load_folders(&mut self) -> Result<Vec<Folder>> {
        Ok(folder::table
            .order(folder::id)
            .select(Folder::as_select())
            .load(&mut self.conn)?)
    }

    pub fn load_items(&mut self, ids: &[ItemId]) -> Result<Catalog> {
        let mut rows = Catalog::default();
        let conn = &mut self.conn;
        for ids in ids.chunks(500) {
            rows.items.extend(
                item::table
                    .filter(item::id.eq_any(ids))
                    .order(item::id)
                    .select(Item::as_select())
                    .load(conn)?,
            );
            rows.sources.extend(
                item_source::table
                    .filter(item_source::item_id.eq_any(ids))
                    .order(item_source::id)
                    .select(ItemSource::as_select())
                    .load(conn)?,
            );
            let files: Vec<ItemFile> = item_file::table
                .filter(item_file::item_id.eq_any(ids))
                .order((item_file::item_id, item_file::blob_id))
                .select(ItemFile::as_select())
                .load(conn)?;
            let blob_ids: Vec<BlobId> = files.iter().map(|file| file.blob_id).collect();
            rows.blobs.extend(
                blob::table
                    .filter(blob::id.eq_any(&blob_ids))
                    .order(blob::id)
                    .select(Blob::as_select())
                    .load(conn)?,
            );
            rows.locations.extend(
                blob_location::table
                    .filter(blob_location::blob_id.eq_any(&blob_ids))
                    .order(blob_location::id)
                    .select(BlobLocation::as_select())
                    .load(conn)?,
            );
            rows.files.extend(files);
            rows.facets.extend(
                facet::table
                    .filter(facet::item_id.eq_any(ids))
                    .order(facet::id)
                    .select(Facet::as_select())
                    .load(conn)?,
            );
            rows.images.extend(
                item_image::table
                    .filter(item_image::item_id.eq_any(ids))
                    .order((item_image::item_id, item_image::position))
                    .select(ItemImage::as_select())
                    .load(conn)?,
            );
            rows.node_tags.extend(
                node_tag::table
                    .filter(node_tag::item_id.eq_any(ids))
                    .order(node_tag::id)
                    .select(NodeTag::as_select())
                    .load(conn)?,
            );
            rows.item_folders.extend(
                item_folder::table
                    .filter(item_folder::item_id.eq_any(ids))
                    .order((item_folder::item_id, item_folder::folder_id))
                    .select(ItemFolder::as_select())
                    .load(conn)?,
            );
        }
        Ok(rows)
    }

    pub fn create_item(&mut self, title: &str) -> Result<ItemId> {
        let now = now_ms();
        Ok(diesel::insert_into(item::table)
            .values((
                item::title.eq(title),
                item::created_at.eq(now),
                item::updated_at.eq(now),
            ))
            .returning(item::id)
            .get_result(&mut self.conn)?)
    }

    pub fn update_item(&mut self, id: ItemId, changes: &ItemChanges) -> Result<()> {
        diesel::update(item::table.find(id))
            .set((changes, item::updated_at.eq(now_ms())))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn add_root(&mut self, path: &str) -> Result<RootId> {
        self.conn.transaction(|conn| {
            diesel::insert_or_ignore_into(root::table)
                .values(root::path.eq(path))
                .execute(conn)?;
            Ok(root::table
                .filter(root::path.eq(path))
                .select(root::id)
                .first(conn)?)
        })
    }

    pub fn add_file(&mut self, file: &FileImport) -> Result<BlobId> {
        self.conn.transaction(|conn| {
            let now = now_ms();
            let existing: Option<(LocationId, BlobId, i64, i64)> = blob_location::table
                .inner_join(blob::table)
                .filter(blob_location::root_id.is(file.root_id))
                .filter(blob_location::rel_path.eq(&file.rel_path))
                .select((blob_location::id, blob::id, blob::size, blob_location::modified_ms))
                .first(conn)
                .optional()?;
            let blob_id = match existing {
                Some((location, blob_id, size, modified_ms))
                    if size == file.size && modified_ms == file.modified_ms =>
                {
                    diesel::update(blob_location::table.find(location))
                        .set((
                            blob_location::modified_ms.eq(file.modified_ms),
                            blob_location::state.eq(LocationState::Present),
                            blob_location::last_seen.eq(now),
                        ))
                        .execute(conn)?;
                    blob_id
                }
                Some((location, ..)) => {
                    let blob_id = insert_blob(conn, file)?;
                    diesel::update(blob_location::table.find(location))
                        .set((
                            blob_location::blob_id.eq(blob_id),
                            blob_location::modified_ms.eq(file.modified_ms),
                            blob_location::state.eq(LocationState::Present),
                            blob_location::last_seen.eq(now),
                        ))
                        .execute(conn)?;
                    blob_id
                }
                None => {
                    let blob_id = insert_blob(conn, file)?;
                    diesel::insert_into(blob_location::table)
                        .values((
                            blob_location::blob_id.eq(blob_id),
                            blob_location::root_id.eq(file.root_id),
                            blob_location::rel_path.eq(&file.rel_path),
                            blob_location::modified_ms.eq(file.modified_ms),
                            blob_location::state.eq(LocationState::Present),
                            blob_location::last_seen.eq(now),
                        ))
                        .execute(conn)?;
                    blob_id
                }
            };
            diesel::insert_or_ignore_into(item_file::table)
                .values(ItemFile {
                    item_id: file.item_id,
                    blob_id,
                    file_name: file.file_name.clone(),
                    display_name: file.display_name.clone(),
                    added_at: now,
                    extracted_from_blob_id: None,
                    extracted_from_path: None,
                })
                .execute(conn)?;
            Ok(blob_id)
        })
    }

    pub fn replace_facets(
        &mut self,
        node: &FacetNode,
        key: FacetKey,
        origin: FacetOrigin,
        values: &[FacetValue],
    ) -> Result<()> {
        self.conn.transaction(|conn| {
            diesel::delete(
                facet::table
                    .filter(facet::item_id.eq(node.item_id))
                    .filter(facet::blob_id.is(node.blob_id))
                    .filter(facet::inner_path.is(node.inner_path.as_deref()))
                    .filter(facet::key.eq(key))
                    .filter(facet::origin.eq(origin))
                    .filter(facet::removed.eq(false)),
            )
            .execute(conn)?;
            if origin == FacetOrigin::User {
                let names: Vec<&str> = values.iter().map(|value| value.value.as_str()).collect();
                diesel::delete(
                    facet::table
                        .filter(facet::item_id.eq(node.item_id))
                        .filter(facet::blob_id.is(node.blob_id))
                        .filter(facet::inner_path.is(node.inner_path.as_deref()))
                        .filter(facet::key.eq(key))
                        .filter(facet::value.eq_any(names)),
                )
                .execute(conn)?;
            }
            for value in values {
                diesel::insert_or_ignore_into(facet::table)
                    .values((
                        facet::item_id.eq(node.item_id),
                        facet::blob_id.eq(node.blob_id),
                        facet::inner_path.eq(node.inner_path.as_deref()),
                        facet::key.eq(key),
                        facet::value.eq(&value.value),
                        facet::target_item_id.eq(value.target_item_id),
                        facet::origin.eq(origin),
                    ))
                    .execute(conn)?;
            }
            Ok(())
        })
    }

    pub fn remove_facet(&mut self, node: &FacetNode, key: FacetKey, value: &str) -> Result<()> {
        self.conn.transaction(|conn| {
            diesel::delete(
                facet::table
                    .filter(facet::item_id.eq(node.item_id))
                    .filter(facet::blob_id.is(node.blob_id))
                    .filter(facet::inner_path.is(node.inner_path.as_deref()))
                    .filter(facet::key.eq(key))
                    .filter(facet::value.eq(value)),
            )
            .execute(conn)?;
            diesel::insert_into(facet::table)
                .values((
                    facet::item_id.eq(node.item_id),
                    facet::blob_id.eq(node.blob_id),
                    facet::inner_path.eq(node.inner_path.as_deref()),
                    facet::key.eq(key),
                    facet::value.eq(value),
                    facet::origin.eq(FacetOrigin::User),
                    facet::removed.eq(true),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    pub fn move_location(&mut self, id: LocationId, rel_path: &str, modified_ms: i64) -> Result<()> {
        diesel::update(blob_location::table.find(id))
            .set((
                blob_location::rel_path.eq(rel_path),
                blob_location::modified_ms.eq(modified_ms),
                blob_location::state.eq(LocationState::Present),
                blob_location::last_seen.eq(now_ms()),
            ))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn set_fingerprint(&mut self, id: BlobId, fingerprint: &str) -> Result<()> {
        diesel::update(blob::table.find(id))
            .set(blob::fingerprint.eq(fingerprint))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn set_crc32(&mut self, id: BlobId, crc32: u32) -> Result<()> {
        diesel::update(blob::table.find(id))
            .set(blob::crc32.eq(i64::from(crc32)))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn mark_missing(&mut self, id: LocationId) -> Result<()> {
        diesel::update(blob_location::table.find(id))
            .set(blob_location::state.eq(LocationState::Missing))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn set_extracted_from(
        &mut self,
        item_id: ItemId,
        blob_id: BlobId,
        from: Option<(BlobId, &str)>,
    ) -> Result<()> {
        diesel::update(item_file::table.find((item_id, blob_id)))
            .set((
                item_file::extracted_from_blob_id.eq(from.map(|(blob, _)| blob)),
                item_file::extracted_from_path.eq(from.map(|(_, path)| path)),
            ))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn remove_item_file(&mut self, item_id: ItemId, blob_id: BlobId) -> Result<()> {
        self.conn.transaction(|conn| {
            diesel::delete(item_file::table.find((item_id, blob_id))).execute(conn)?;
            let referenced: i64 = item_file::table
                .filter(item_file::blob_id.eq(blob_id).or(item_file::extracted_from_blob_id.eq(blob_id)))
                .count()
                .get_result(conn)?;
            if referenced == 0 {
                diesel::delete(blob::table.find(blob_id)).execute(conn)?;
            }
            Ok(())
        })
    }

    pub fn tag_node(&mut self, node: &FacetNode, name: &str, key: &str) -> Result<TagId> {
        self.conn.transaction(|conn| {
            diesel::insert_or_ignore_into(tag::table)
                .values((tag::name.eq(name), tag::key.eq(key)))
                .execute(conn)?;
            let tag_id: TagId = tag::table
                .filter(tag::key.eq(key))
                .select(tag::id)
                .first(conn)?;
            diesel::insert_or_ignore_into(node_tag::table)
                .values((
                    node_tag::item_id.eq(node.item_id),
                    node_tag::blob_id.eq(node.blob_id),
                    node_tag::inner_path.eq(node.inner_path.as_deref()),
                    node_tag::tag_id.eq(tag_id),
                ))
                .execute(conn)?;
            Ok(tag_id)
        })
    }

    pub fn untag_node(&mut self, node: &FacetNode, tag_id: TagId) -> Result<()> {
        diesel::delete(
            node_tag::table
                .filter(node_tag::item_id.eq(node.item_id))
                .filter(node_tag::blob_id.is(node.blob_id))
                .filter(node_tag::inner_path.is(node.inner_path.as_deref()))
                .filter(node_tag::tag_id.eq(tag_id)),
        )
        .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn create_folder(&mut self, parent_id: Option<FolderId>, name: &str) -> Result<FolderId> {
        self.conn.transaction(|conn| {
            let last: Option<i32> = folder::table
                .filter(folder::parent_id.is(parent_id))
                .select(max(folder::position))
                .first(conn)?;
            Ok(diesel::insert_into(folder::table)
                .values((
                    folder::parent_id.eq(parent_id),
                    folder::name.eq(name),
                    folder::position.eq(last.map_or(0, |last| last + 1)),
                ))
                .returning(folder::id)
                .get_result(conn)?)
        })
    }

    pub fn put_in_folder(&mut self, item_id: ItemId, folder_id: FolderId) -> Result<()> {
        diesel::insert_or_ignore_into(item_folder::table)
            .values(ItemFolder { item_id, folder_id })
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn remove_from_folder(&mut self, item_id: ItemId, folder_id: FolderId) -> Result<()> {
        diesel::delete(item_folder::table.find((item_id, folder_id))).execute(&mut self.conn)?;
        Ok(())
    }

    pub fn remove_root(&mut self, root: RootId) -> Result<()> {
        const ONLY_HERE: &str = "SELECT blob_id FROM blob_location WHERE root_id = ?1
             EXCEPT SELECT blob_id FROM blob_location WHERE root_id IS NOT ?1";
        let steps = [
            format!(
                "DELETE FROM item WHERE id IN (SELECT item_id FROM item_file WHERE blob_id IN ({ONLY_HERE}))
                 AND id NOT IN (SELECT item_id FROM item_file WHERE blob_id NOT IN ({ONLY_HERE}))"
            ),
            format!("DELETE FROM item_file WHERE blob_id IN ({ONLY_HERE})"),
            format!("DELETE FROM blob WHERE id IN ({ONLY_HERE}) AND id NOT IN (SELECT blob_id FROM item_file)"),
            "DELETE FROM blob_location WHERE root_id = ?1".to_owned(),
            "DELETE FROM root WHERE id = ?1".to_owned(),
        ];
        self.conn.transaction(|conn| {
            for step in &steps {
                diesel::sql_query(step).bind::<BigInt, _>(root.0).execute(conn)?;
            }
            Ok::<_, diesel::result::Error>(())
        })?;
        Ok(())
    }

    pub fn merge_items(&mut self, into: ItemId, from: ItemId) -> Result<()> {
        const STEPS: [&str; 7] = [
            "INSERT OR IGNORE INTO item_file
                 (item_id, blob_id, file_name, display_name, added_at, extracted_from_blob_id, extracted_from_path)
             SELECT ?1, blob_id, file_name, display_name, added_at, extracted_from_blob_id, extracted_from_path
             FROM item_file WHERE item_id = ?2",
            "UPDATE OR IGNORE facet SET item_id = ?1
             WHERE item_id = ?2 AND (blob_id IS NOT NULL OR (origin = 'user' AND NOT EXISTS (
                 SELECT 1 FROM facet AS kept
                 WHERE kept.item_id = ?1 AND kept.blob_id IS NULL AND kept.key = facet.key AND kept.origin = 'user')))",
            "UPDATE OR IGNORE node_tag SET item_id = ?1 WHERE item_id = ?2",
            "INSERT OR IGNORE INTO item_folder (item_id, folder_id) SELECT ?1, folder_id FROM item_folder WHERE item_id = ?2",
            "UPDATE item_source SET item_id = ?1 WHERE item_id = ?2",
            "UPDATE facet SET target_item_id = ?1 WHERE target_item_id = ?2",
            "DELETE FROM item WHERE id = ?2 AND ?1 <> ?2",
        ];
        self.conn.transaction(|conn| {
            for step in STEPS {
                diesel::sql_query(step)
                    .bind::<BigInt, _>(into.0)
                    .bind::<BigInt, _>(from.0)
                    .execute(conn)?;
            }
            Ok::<_, diesel::result::Error>(())
        })?;
        Ok(())
    }
}

fn insert_blob(conn: &mut SqliteConnection, file: &FileImport) -> QueryResult<BlobId> {
    diesel::insert_into(blob::table)
        .values((
            blob::size.eq(file.size),
            blob::crc32.eq(file.crc32),
            blob::kind.eq(file.kind),
        ))
        .returning(blob::id)
        .get_result(conn)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests;
