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

#[test]
fn open_creates_the_schema_and_reopening_keeps_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let mut store = Store::open(dir.path()).unwrap();
        store
            .create_item("Gloves", ItemStatus::Inbox)
            .unwrap()
    };

    let mut store = Store::open(dir.path()).unwrap();
    let catalog = store.load().unwrap();

    assert_eq!(catalog.items.len(), 1);
    let item = &catalog.items[0];
    assert_eq!((item.id, item.title.as_str()), (id, "Gloves"));
    assert_eq!(item.status, ItemStatus::Inbox);
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
        .create_item("Earrings", ItemStatus::Confirmed)
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
        .create_item("gloves", ItemStatus::Inbox)
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
    assert!(item.updated_at >= item.created_at);
}

#[test]
fn add_file_reuses_the_blob_for_a_known_path_and_shares_it_across_versions() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", ItemStatus::Inbox)
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
        .create_item("Gloves", ItemStatus::Inbox)
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
fn a_blob_referenced_by_a_version_cannot_be_deleted() {
    let (_dir, mut store) = open();
    let item = store
        .create_item("Gloves", ItemStatus::Inbox)
        .unwrap();
    let version = store.add_version(item, None, None).unwrap();
    let blob_id = store.add_file(&import(version, "D:/Gloves.unitypackage", "10:1")).unwrap();

    let result = diesel::delete(blob::table.find(blob_id)).execute(&mut store.conn);

    assert!(result.is_err());
}

#[test]
fn tags_are_unique_case_insensitively_and_shared_between_items() {
    let (_dir, mut store) = open();
    let a = store.create_item("A", ItemStatus::Inbox).unwrap();
    let b = store.create_item("B", ItemStatus::Inbox).unwrap();

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
fn an_unknown_enum_value_fails_to_load_instead_of_defaulting() {
    let (_dir, mut store) = open();
    store.create_item("Hair", ItemStatus::Inbox).unwrap();
    diesel::sql_query("UPDATE item SET status = 'spaceship'")
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
        chain.iter().any(|message| message == "unknown ItemStatus value \"spaceship\""),
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
    check(StoreKind::ALL, StoreKind::as_str);
    check(LocationState::ALL, LocationState::as_str);
    check(ImageOrigin::ALL, ImageOrigin::as_str);
    check(FileRole::ALL, FileRole::as_str);
    check(BlobKind::ALL, BlobKind::as_str);
}
