use std::fmt;
use std::str::FromStr;

use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::prelude::*;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::Sqlite;
use snowcase_formats::FileKind;

use crate::schema::*;

macro_rules! id_type {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, AsExpression, FromSqlRow)]
        #[diesel(sql_type = BigInt)]
        pub struct $name(pub i64);

        impl ToSql<BigInt, Sqlite> for $name {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                <i64 as ToSql<BigInt, Sqlite>>::to_sql(&self.0, out)
            }
        }

        impl FromSql<BigInt, Sqlite> for $name {
            fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
                <i64 as FromSql<BigInt, Sqlite>>::from_sql(value).map(Self)
            }
        }
    )*};
}

id_type!(ItemId, SourceId, VersionId, BlobId, LocationId, TagId, FolderId);

#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} value {value:?}")]
pub struct UnknownValue {
    pub kind: &'static str,
    pub value: String,
}

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsExpression, FromSqlRow)]
        #[diesel(sql_type = Text)]
        pub enum $name {
            $($variant),*
        }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant),*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),*
                }
            }
        }

        impl FromStr for $name {
            type Err = UnknownValue;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok(Self::$variant),)*
                    _ => Err(UnknownValue {
                        kind: stringify!($name),
                        value: value.to_owned(),
                    }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ToSql<Text, Sqlite> for $name {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                out.set_value(self.as_str());
                Ok(IsNull::No)
            }
        }

        impl FromSql<Text, Sqlite> for $name {
            fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
                let text = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
                Ok(text.parse()?)
            }
        }
    };
}

text_enum!(ItemStatus {
    Inbox => "inbox",
    Confirmed => "confirmed",
});

text_enum!(Category {
    Avatar => "avatar",
    Outfit => "outfit",
    Accessory => "accessory",
    Hair => "hair",
    Shader => "shader",
    Tool => "tool",
    Texture => "texture",
    Other => "other",
});

text_enum!(StoreKind {
    Booth => "booth",
    Other => "other",
    Local => "local",
});

text_enum!(LocationState {
    Present => "present",
    Missing => "missing",
    Archived => "archived",
});

text_enum!(ImageOrigin {
    User => "user",
    Store => "store",
    Package => "package",
});

text_enum!(FileRole {
    Package => "package",
    Texture => "texture",
    Source => "source",
    Document => "document",
    Sample => "sample",
    Other => "other",
});

text_enum!(BlobKind {
    UnityPackage => "unitypackage",
    Archive => "archive",
    Image => "image",
    Model => "model",
    Other => "other",
});

impl From<FileKind> for BlobKind {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::UnityPackage => Self::UnityPackage,
            FileKind::Archive => Self::Archive,
            FileKind::Image => Self::Image,
            FileKind::Model => Self::Model,
            FileKind::Other => Self::Other,
        }
    }
}

impl From<BlobKind> for FileKind {
    fn from(kind: BlobKind) -> Self {
        match kind {
            BlobKind::UnityPackage => Self::UnityPackage,
            BlobKind::Archive => Self::Archive,
            BlobKind::Image => Self::Image,
            BlobKind::Model => Self::Model,
            BlobKind::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = item, check_for_backend(Sqlite))]
pub struct Item {
    pub id: ItemId,
    pub title: String,
    pub category: Category,
    pub status: ItemStatus,
    pub note: String,
    pub cover_media: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, AsChangeset)]
#[diesel(table_name = item)]
pub struct ItemChanges {
    pub title: Option<String>,
    pub category: Option<Category>,
    pub status: Option<ItemStatus>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = item_source, check_for_backend(Sqlite))]
pub struct ItemSource {
    pub id: SourceId,
    pub item_id: ItemId,
    pub store: StoreKind,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub author: Option<String>,
    pub account: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Insertable)]
#[diesel(table_name = item_source)]
pub struct NewSource {
    pub item_id: ItemId,
    pub store: StoreKind,
    pub url: Option<String>,
    pub external_id: Option<String>,
    pub author: Option<String>,
    pub account: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = version, check_for_backend(Sqlite))]
pub struct Version {
    pub id: VersionId,
    pub item_id: ItemId,
    pub label: Option<String>,
    pub released_at: Option<i64>,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = blob, check_for_backend(Sqlite))]
pub struct Blob {
    pub id: BlobId,
    pub hash: Option<String>,
    pub size: i64,
    pub fingerprint: String,
    pub kind: BlobKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = blob_location, check_for_backend(Sqlite))]
pub struct BlobLocation {
    pub id: LocationId,
    pub blob_id: BlobId,
    pub path: String,
    pub state: LocationState,
    pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = version_file, check_for_backend(Sqlite))]
pub struct VersionFile {
    pub version_id: VersionId,
    pub blob_id: BlobId,
    pub file_name: String,
    pub part: Option<String>,
    pub variant: Option<String>,
    pub role: Option<FileRole>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImport {
    pub version_id: VersionId,
    pub path: String,
    pub file_name: String,
    pub size: i64,
    pub fingerprint: String,
    pub kind: BlobKind,
    pub part: Option<String>,
    pub variant: Option<String>,
    pub role: Option<FileRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = media, check_for_backend(Sqlite))]
pub struct Media {
    pub hash: String,
    pub mime: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = item_image, check_for_backend(Sqlite))]
pub struct ItemImage {
    pub item_id: ItemId,
    pub media_hash: String,
    pub position: i32,
    pub origin: ImageOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = tag, check_for_backend(Sqlite))]
pub struct Tag {
    pub id: TagId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = item_tag, check_for_backend(Sqlite))]
pub struct ItemTag {
    pub item_id: ItemId,
    pub tag_id: TagId,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = folder, check_for_backend(Sqlite))]
pub struct Folder {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = item_folder, check_for_backend(Sqlite))]
pub struct ItemFolder {
    pub item_id: ItemId,
    pub folder_id: FolderId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub items: Vec<Item>,
    pub sources: Vec<ItemSource>,
    pub versions: Vec<Version>,
    pub version_files: Vec<VersionFile>,
    pub blobs: Vec<Blob>,
    pub locations: Vec<BlobLocation>,
    pub media: Vec<Media>,
    pub images: Vec<ItemImage>,
    pub tags: Vec<Tag>,
    pub item_tags: Vec<ItemTag>,
    pub folders: Vec<Folder>,
    pub item_folders: Vec<ItemFolder>,
}
