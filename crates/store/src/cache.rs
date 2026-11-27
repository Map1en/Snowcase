use std::collections::HashMap;
use std::fs;
use std::path::Path;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::Integer;
use diesel::sqlite::SqliteConnection;
use snowcase_formats::PackageAsset;

use crate::{Result, StoreError};

const CACHE: &str = "cache.db";
const VERSION: i32 = 1;

diesel::table! {
    package (fingerprint, member) {
        fingerprint -> Text,
        member -> Text,
        error -> Nullable<Text>,
    }
}

diesel::table! {
    package_asset (fingerprint, member, guid) {
        fingerprint -> Text,
        member -> Text,
        guid -> Text,
        path -> Text,
        is_file -> Bool,
        has_preview -> Bool,
    }
}

diesel::table! {
    license (fingerprint, member) {
        fingerprint -> Text,
        member -> Text,
        parser -> Integer,
        vn3 -> Nullable<Text>,
        error -> Nullable<Text>,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageKey {
    pub fingerprint: String,
    pub member: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedPackage {
    pub key: PackageKey,
    pub result: Result<Vec<PackageAsset>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedLicense {
    pub key: PackageKey,
    pub parser: i32,
    pub result: Result<Option<String>, String>,
}

#[derive(QueryableByName)]
struct UserVersion {
    #[diesel(sql_type = Integer)]
    user_version: i32,
}

pub struct Cache {
    conn: SqliteConnection,
}

impl Cache {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(CACHE);
        let url = path.to_str().ok_or_else(|| StoreError::NonUtf8Path(path.clone()))?.to_owned();
        let mut conn = SqliteConnection::establish(&url)?;
        let version = diesel::sql_query("PRAGMA user_version")
            .get_result::<UserVersion>(&mut conn)?
            .user_version;
        if version != 0 && version != VERSION {
            drop(conn);
            Self::remove(dir)?;
            conn = SqliteConnection::establish(&url)?;
        }
        conn.batch_execute(&format!(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS package (
                 fingerprint TEXT NOT NULL,
                 member TEXT NOT NULL,
                 error TEXT,
                 PRIMARY KEY (fingerprint, member)
             );
             CREATE TABLE IF NOT EXISTS package_asset (
                 fingerprint TEXT NOT NULL,
                 member TEXT NOT NULL,
                 guid TEXT NOT NULL,
                 path TEXT NOT NULL,
                 is_file BOOLEAN NOT NULL,
                 has_preview BOOLEAN NOT NULL,
                 PRIMARY KEY (fingerprint, member, guid)
             );
             CREATE TABLE IF NOT EXISTS license (
                 fingerprint TEXT NOT NULL,
                 member TEXT NOT NULL,
                 parser INTEGER NOT NULL,
                 vn3 TEXT,
                 error TEXT,
                 PRIMARY KEY (fingerprint, member)
             );
             PRAGMA user_version = {VERSION};"
        ))?;
        Ok(Self { conn })
    }

    pub fn remove(dir: &Path) -> Result<()> {
        for suffix in ["", "-wal", "-shm"] {
            let file = dir.join(format!("{CACHE}{suffix}"));
            if file.exists() {
                fs::remove_file(file)?;
            }
        }
        Ok(())
    }

    pub fn save_packages(&mut self, scanned: &[ScannedPackage]) -> Result<()> {
        self.conn.transaction(|conn| {
            for scan in scanned {
                let key = &scan.key;
                diesel::replace_into(package::table)
                    .values((
                        package::fingerprint.eq(&key.fingerprint),
                        package::member.eq(&key.member),
                        package::error.eq(scan.result.as_ref().err()),
                    ))
                    .execute(conn)?;
                diesel::delete(
                    package_asset::table
                        .filter(package_asset::fingerprint.eq(&key.fingerprint))
                        .filter(package_asset::member.eq(&key.member)),
                )
                .execute(conn)?;
                for asset in scan.result.iter().flatten() {
                    diesel::insert_into(package_asset::table)
                        .values((
                            package_asset::fingerprint.eq(&key.fingerprint),
                            package_asset::member.eq(&key.member),
                            package_asset::guid.eq(&asset.guid),
                            package_asset::path.eq(&asset.path),
                            package_asset::is_file.eq(asset.is_file),
                            package_asset::has_preview.eq(asset.has_preview),
                        ))
                        .execute(conn)?;
                }
            }
            Ok::<_, diesel::result::Error>(())
        })?;
        Ok(())
    }

    pub fn save_licenses(&mut self, scanned: &[ScannedLicense]) -> Result<()> {
        self.conn.transaction(|conn| {
            for scan in scanned {
                let (vn3, error) = match &scan.result {
                    Ok(vn3) => (vn3.as_deref(), None),
                    Err(error) => (None, Some(error.as_str())),
                };
                diesel::replace_into(license::table)
                    .values((
                        license::fingerprint.eq(&scan.key.fingerprint),
                        license::member.eq(&scan.key.member),
                        license::parser.eq(scan.parser),
                        license::vn3.eq(vn3),
                        license::error.eq(error),
                    ))
                    .execute(conn)?;
            }
            Ok::<_, diesel::result::Error>(())
        })?;
        Ok(())
    }

    pub fn load_licenses(&mut self) -> Result<Vec<ScannedLicense>> {
        type Row = (String, String, i32, Option<String>, Option<String>);
        let rows: Vec<Row> = license::table
            .order((license::fingerprint, license::member))
            .load(&mut self.conn)?;
        Ok(rows
            .into_iter()
            .map(|(fingerprint, member, parser, vn3, error)| ScannedLicense {
                key: PackageKey { fingerprint, member },
                parser,
                result: error.map_or(Ok(vn3), Err),
            })
            .collect())
    }

    pub fn load_packages(&mut self) -> Result<Vec<ScannedPackage>> {
        let packages: Vec<(String, String, Option<String>)> = package::table
            .order((package::fingerprint, package::member))
            .load(&mut self.conn)?;
        let assets: Vec<(String, String, String, String, bool, bool)> = package_asset::table
            .order((package_asset::fingerprint, package_asset::member, package_asset::guid))
            .load(&mut self.conn)?;
        let mut grouped: HashMap<(String, String), Vec<PackageAsset>> = HashMap::new();
        for (fingerprint, member, guid, path, is_file, has_preview) in assets {
            grouped.entry((fingerprint, member)).or_default().push(PackageAsset {
                guid,
                path,
                is_file,
                has_preview,
            });
        }
        Ok(packages
            .into_iter()
            .map(|(fingerprint, member, error)| {
                let found = grouped.remove(&(fingerprint.clone(), member.clone())).unwrap_or_default();
                ScannedPackage {
                    key: PackageKey { fingerprint, member },
                    result: error.map_or(Ok(found), Err),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
