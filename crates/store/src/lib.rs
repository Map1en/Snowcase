mod model;
mod schema;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use diesel::connection::SimpleConnection;
use diesel::dsl::max;
use diesel::prelude::*;
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

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

    pub fn load(&mut self) -> Result<Catalog> {
        let conn = &mut self.conn;
        Ok(Catalog {
            items: item::table.order(item::id).select(Item::as_select()).load(conn)?,
            sources: item_source::table
                .order(item_source::id)
                .select(ItemSource::as_select())
                .load(conn)?,
            versions: version::table
                .order(version::id)
                .select(Version::as_select())
                .load(conn)?,
            version_files: version_file::table
                .order((version_file::version_id, version_file::blob_id))
                .select(VersionFile::as_select())
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
            tags: tag::table.order(tag::id).select(Tag::as_select()).load(conn)?,
            item_tags: item_tag::table
                .order((item_tag::item_id, item_tag::tag_id))
                .select(ItemTag::as_select())
                .load(conn)?,
            folders: folder::table
                .order(folder::id)
                .select(Folder::as_select())
                .load(conn)?,
            item_folders: item_folder::table
                .order((item_folder::item_id, item_folder::folder_id))
                .select(ItemFolder::as_select())
                .load(conn)?,
        })
    }

    pub fn create_item(&mut self, title: &str, status: ItemStatus) -> Result<ItemId> {
        let now = now_ms();
        Ok(diesel::insert_into(item::table)
            .values((
                item::title.eq(title),
                item::status.eq(status),
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

    pub fn add_version(
        &mut self,
        item_id: ItemId,
        label: Option<&str>,
        released_at: Option<i64>,
    ) -> Result<VersionId> {
        Ok(diesel::insert_into(version::table)
            .values((
                version::item_id.eq(item_id),
                version::label.eq(label),
                version::released_at.eq(released_at),
                version::added_at.eq(now_ms()),
            ))
            .returning(version::id)
            .get_result(&mut self.conn)?)
    }

    pub fn add_file(&mut self, file: &FileImport) -> Result<BlobId> {
        self.conn.immediate_transaction(|conn| {
            let now = now_ms();
            let existing: Option<(LocationId, BlobId, String)> = blob_location::table
                .inner_join(blob::table)
                .filter(blob_location::path.eq(&file.path))
                .select((blob_location::id, blob::id, blob::fingerprint))
                .first(conn)
                .optional()?;
            let blob_id = match existing {
                Some((location, blob_id, fingerprint)) if fingerprint == file.fingerprint => {
                    diesel::update(blob_location::table.find(location))
                        .set((
                            blob_location::state.eq(LocationState::Present),
                            blob_location::last_seen.eq(now),
                        ))
                        .execute(conn)?;
                    blob_id
                }
                Some((location, _, _)) => {
                    let blob_id = insert_blob(conn, file)?;
                    diesel::update(blob_location::table.find(location))
                        .set((
                            blob_location::blob_id.eq(blob_id),
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
                            blob_location::path.eq(&file.path),
                            blob_location::state.eq(LocationState::Present),
                            blob_location::last_seen.eq(now),
                        ))
                        .execute(conn)?;
                    blob_id
                }
            };
            diesel::insert_or_ignore_into(version_file::table)
                .values(VersionFile {
                    version_id: file.version_id,
                    blob_id,
                    file_name: file.file_name.clone(),
                    part: file.part.clone(),
                    variant: file.variant.clone(),
                    role: file.role,
                })
                .execute(conn)?;
            Ok(blob_id)
        })
    }

    pub fn tag_item(&mut self, item_id: ItemId, name: &str) -> Result<TagId> {
        self.conn.immediate_transaction(|conn| {
            diesel::insert_or_ignore_into(tag::table)
                .values(tag::name.eq(name))
                .execute(conn)?;
            let tag_id: TagId = tag::table
                .filter(tag::name.eq(name))
                .select(tag::id)
                .first(conn)?;
            diesel::insert_or_ignore_into(item_tag::table)
                .values(ItemTag { item_id, tag_id })
                .execute(conn)?;
            Ok(tag_id)
        })
    }

    pub fn untag_item(&mut self, item_id: ItemId, tag_id: TagId) -> Result<()> {
        diesel::delete(item_tag::table.find((item_id, tag_id))).execute(&mut self.conn)?;
        Ok(())
    }

    pub fn create_folder(&mut self, parent_id: Option<FolderId>, name: &str) -> Result<FolderId> {
        self.conn.immediate_transaction(|conn| {
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

}

fn insert_blob(conn: &mut SqliteConnection, file: &FileImport) -> QueryResult<BlobId> {
    diesel::insert_into(blob::table)
        .values((
            blob::size.eq(file.size),
            blob::fingerprint.eq(&file.fingerprint),
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
