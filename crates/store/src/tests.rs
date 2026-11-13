use super::*;

fn open() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn import(version_id: VersionId, path: &str, fingerprint: &str) -> FileImport {
    FileImport {
        version_id,
        path: path.to_owned(),
        file_name: path.rsplit('/').next().unwrap().to_owned(),
        size: 10,
        fingerprint: fingerprint.to_owned(),
        kind: BlobKind::UnityPackage,
        part: None,
        variant: None,
        role: Some(FileRole::Package),
    }
}

fn media(hash: &str) -> Media {
    Media {
        hash: hash.to_owned(),
        mime: "image/webp".to_owned(),
        width: Some(1600),
        height: Some(900),
        bytes: 1234,
    }
}

#[test]
fn open_creates_the_schema_and_reopening_keeps_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let mut store = Store::open(dir.path()).unwrap();
        store
            .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
            .unwrap()
    };

    let mut store = Store::open(dir.path()).unwrap();
    let catalog = store.load().unwrap();

    assert_eq!(catalog.items.len(), 1);
    let item = &catalog.items[0];
    assert_eq!((item.id, item.title.as_str()), (id, "Gloves"));
    assert_eq!((item.category, item.status), (Category::Accessory, ItemStatus::Inbox));
    assert_eq!(item.note, "");
    assert!(!dir.path().join(BACKUPS).exists());
}

#[test]
fn open_refuses_a_library_written_by_a_newer_version() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut store = Store::open(dir.path()).unwrap();
        diesel::sql_query(
            "INSERT INTO __diesel_schema_migrations (version) VALUES ('99990101000000')",
        )
        .execute(&mut store.conn)
        .unwrap();
    }

    match Store::open(dir.path()) {
        Err(StoreError::NewerLibrary(unknown)) => assert_eq!(unknown, ["99990101000000"]),
        other => panic!("expected NewerLibrary, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn backup_writes_a_database_that_opens_with_the_same_data() {
    let (_dir, mut store) = open();
    store
        .create_item("Earrings", Category::Accessory, ItemStatus::Confirmed)
        .unwrap();

    let backup = store.backup().unwrap();
    let second = store.backup().unwrap();
    assert_ne!(backup, second);

    let restored = tempfile::tempdir().unwrap();
    fs::copy(&backup, restored.path().join(DATABASE)).unwrap();
    let catalog = Store::open(restored.path()).unwrap().load().unwrap();
    assert_eq!(catalog.items[0].title, "Earrings");
}

#[test]
fn update_item_applies_only_the_given_fields() {
    let (_dir, mut store) = open();
    let id = store
        .create_item("gloves", Category::Other, ItemStatus::Inbox)
        .unwrap();

    store
        .update_item(
            id,
            &ItemChanges {
                title: Some("Gloves".into()),
                status: Some(ItemStatus::Confirmed),
                ..Default::default()
            },
        )
        .unwrap();

    let item = store.load().unwrap().items.remove(0);
    assert_eq!(item.title, "Gloves");
    assert_eq!(item.status, ItemStatus::Confirmed);
    assert_eq!(item.category, Category::Other);
    assert!(item.updated_at >= item.created_at);
}

#[test]
fn add_file_reuses_the_blob_for_a_known_path_and_shares_it_across_versions() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
        .unwrap();
    let v1 = store.add_version(item, Some("1.0"), None).unwrap();
    let v2 = store.add_version(item, Some("1.1"), None).unwrap();

    let first = store.add_file(&import(v1, "D:/BOOTH/Gloves.unitypackage", "10:1")).unwrap();
    let again = store.add_file(&import(v1, "D:/BOOTH/Gloves.unitypackage", "10:1")).unwrap();
    let shared = store.add_file(&import(v2, "D:/BOOTH/Gloves.unitypackage", "10:1")).unwrap();

    assert_eq!(first, again);
    assert_eq!(first, shared);
    let catalog = store.load().unwrap();
    assert_eq!(catalog.blobs.len(), 1);
    assert_eq!(catalog.locations.len(), 1);
    assert_eq!(catalog.version_files.len(), 2);
}

#[test]
fn add_file_creates_a_new_blob_when_the_file_at_a_path_changed() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
        .unwrap();
    let v1 = store.add_version(item, Some("1.0"), None).unwrap();
    let v2 = store.add_version(item, Some("1.1"), None).unwrap();

    let old = store.add_file(&import(v1, "D:/BOOTH/Gloves.unitypackage", "10:1")).unwrap();
    let new = store.add_file(&import(v2, "D:/BOOTH/Gloves.unitypackage", "11:2")).unwrap();

    assert_ne!(old, new);
    let catalog = store.load().unwrap();
    assert_eq!(catalog.blobs.len(), 2);
    assert_eq!(catalog.locations.len(), 1);
    assert_eq!(catalog.locations[0].blob_id, new);
}

#[test]
fn set_blob_hash_merges_duplicates_into_the_existing_blob() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
        .unwrap();
    let v1 = store.add_version(item, Some("1.0"), None).unwrap();
    let v2 = store.add_version(item, Some("1.1"), None).unwrap();
    let a = store.add_file(&import(v1, "D:/A/Gloves.unitypackage", "10:1")).unwrap();
    let b = store.add_file(&import(v1, "D:/B/Gloves.unitypackage", "10:2")).unwrap();
    store.add_file(&import(v2, "D:/B/Gloves.unitypackage", "10:2")).unwrap();

    assert_eq!(store.set_blob_hash(a, "same").unwrap(), a);
    assert_eq!(store.set_blob_hash(b, "same").unwrap(), a);

    let catalog = store.load().unwrap();
    assert_eq!(catalog.blobs.len(), 1);
    assert_eq!(catalog.blobs[0].hash.as_deref(), Some("same"));
    assert!(catalog.locations.iter().all(|location| location.blob_id == a));
    let files: Vec<_> = catalog
        .version_files
        .iter()
        .map(|file| (file.version_id, file.blob_id))
        .collect();
    assert_eq!(files, [(v1, a), (v2, a)]);
}

#[test]
fn deleting_an_item_cascades_but_keeps_blobs_and_media() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
        .unwrap();
    let version = store.add_version(item, None, None).unwrap();
    store.add_file(&import(version, "D:/Gloves.unitypackage", "10:1")).unwrap();
    store
        .add_source(&NewSource {
            item_id: item,
            store: StoreKind::Booth,
            url: Some("https://booth.pm/ja/items/123456".into()),
            external_id: Some("123456".into()),
            author: None,
            account: None,
        })
        .unwrap();
    store.add_media(&media("cover")).unwrap();
    store.add_item_image(item, "cover", ImageOrigin::Store).unwrap();
    store.tag_item(item, "gloves").unwrap();
    let folder = store.create_folder(None, "Accessories").unwrap();
    store.put_in_folder(item, folder).unwrap();

    store.delete_item(item).unwrap();

    let catalog = store.load().unwrap();
    assert!(catalog.items.is_empty());
    assert!(catalog.sources.is_empty());
    assert!(catalog.versions.is_empty());
    assert!(catalog.version_files.is_empty());
    assert!(catalog.images.is_empty());
    assert!(catalog.item_tags.is_empty());
    assert!(catalog.item_folders.is_empty());
    assert_eq!((catalog.blobs.len(), catalog.locations.len()), (1, 1));
    assert_eq!((catalog.media.len(), catalog.tags.len(), catalog.folders.len()), (1, 1, 1));
}

#[test]
fn a_blob_referenced_by_a_version_cannot_be_deleted() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", Category::Accessory, ItemStatus::Inbox)
        .unwrap();
    let version = store.add_version(item, None, None).unwrap();
    let blob_id = store.add_file(&import(version, "D:/Gloves.unitypackage", "10:1")).unwrap();

    let result = diesel::delete(blob::table.find(blob_id)).execute(&mut store.conn);

    assert!(result.is_err());
}

#[test]
fn tags_are_unique_case_insensitively_and_shared_between_items() {
    let (_dir, mut store) = open();
    let a = store.create_item("A", Category::Outfit, ItemStatus::Inbox).unwrap();
    let b = store.create_item("B", Category::Outfit, ItemStatus::Inbox).unwrap();

    let first = store.tag_item(a, "Kikyo").unwrap();
    let second = store.tag_item(b, "kikyo").unwrap();
    assert_eq!(first, second);
    store.tag_item(a, "kikyo").unwrap();
    store.untag_item(b, second).unwrap();

    let catalog = store.load().unwrap();
    assert_eq!(catalog.tags.len(), 1);
    assert_eq!(catalog.tags[0].name, "Kikyo");
    assert_eq!(catalog.item_tags, [ItemTag { item_id: a, tag_id: first }]);
}

#[test]
fn folders_are_positioned_per_parent() {
    let (_dir, mut store) = open();
    let avatars = store.create_folder(None, "Avatars").unwrap();
    let shaders = store.create_folder(None, "Shaders").unwrap();
    let kikyo = store.create_folder(Some(avatars), "Kikyo").unwrap();
    let manuka = store.create_folder(Some(avatars), "Manuka").unwrap();

    let catalog = store.load().unwrap();
    let positions: Vec<_> = catalog
        .folders
        .iter()
        .map(|folder| (folder.id, folder.parent_id, folder.position))
        .collect();
    assert_eq!(
        positions,
        [
            (avatars, None, 0),
            (shaders, None, 1),
            (kikyo, Some(avatars), 0),
            (manuka, Some(avatars), 1),
        ]
    );
}

#[test]
fn item_images_keep_their_order_and_the_cover_can_be_cleared() {
    let (_dir, mut store) = open();
    let item = store.create_item("Hair", Category::Hair, ItemStatus::Inbox).unwrap();
    for hash in ["a", "b"] {
        store.add_media(&media(hash)).unwrap();
    }
    store.add_media(&media("a")).unwrap();
    store.add_item_image(item, "a", ImageOrigin::Store).unwrap();
    store.add_item_image(item, "b", ImageOrigin::User).unwrap();
    store.add_item_image(item, "a", ImageOrigin::User).unwrap();
    store.set_cover(item, Some("b")).unwrap();
    assert_eq!(store.load().unwrap().items[0].cover_media.as_deref(), Some("b"));

    store.set_cover(item, None).unwrap();

    let catalog = store.load().unwrap();
    assert_eq!(catalog.items[0].cover_media, None);
    assert_eq!(catalog.media.len(), 2);
    let images: Vec<_> = catalog
        .images
        .iter()
        .map(|image| (image.media_hash.as_str(), image.position, image.origin))
        .collect();
    assert_eq!(images, [("a", 0, ImageOrigin::Store), ("b", 1, ImageOrigin::User)]);
}

#[test]
fn set_location_state_marks_missing_files() {
    let (_dir, mut store) = open();
    let item = store.create_item("Hair", Category::Hair, ItemStatus::Inbox).unwrap();
    let version = store.add_version(item, None, None).unwrap();
    store.add_file(&import(version, "D:/Hair.zip", "10:1")).unwrap();

    assert!(store.set_location_state("D:/Hair.zip", LocationState::Missing).unwrap());
    assert!(!store.set_location_state("D:/Other.zip", LocationState::Missing).unwrap());

    assert_eq!(store.load().unwrap().locations[0].state, LocationState::Missing);
}

#[test]
fn an_unknown_enum_value_fails_to_load_instead_of_defaulting() {
    let (_dir, mut store) = open();
    store.create_item("Hair", Category::Hair, ItemStatus::Inbox).unwrap();
    diesel::sql_query("UPDATE item SET category = 'spaceship'")
        .execute(&mut store.conn)
        .unwrap();

    let error = store.load().unwrap_err();

    let mut chain = Vec::new();
    let mut cause: Option<&dyn std::error::Error> = Some(&error);
    while let Some(current) = cause {
        chain.push(current.to_string());
        cause = current.source();
    }
    assert!(
        chain.iter().any(|message| message == "unknown Category value \"spaceship\""),
        "{chain:?}"
    );
}

#[test]
fn text_enums_round_trip_through_their_text() {
    fn check<T: Copy + PartialEq + std::fmt::Debug + std::str::FromStr>(all: &[T], text: fn(T) -> &'static str)
    where
        T::Err: std::fmt::Debug,
    {
        for value in all {
            assert_eq!(text(*value).parse::<T>().unwrap(), *value);
        }
    }
    check(ItemStatus::ALL, ItemStatus::as_str);
    check(Category::ALL, Category::as_str);
    check(StoreKind::ALL, StoreKind::as_str);
    check(LocationState::ALL, LocationState::as_str);
    check(ImageOrigin::ALL, ImageOrigin::as_str);
    check(FileRole::ALL, FileRole::as_str);
    check(BlobKind::ALL, BlobKind::as_str);
}
