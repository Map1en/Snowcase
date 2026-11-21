use super::*;

fn open() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn import(item_id: ItemId, root_id: Option<RootId>, rel_path: &str, fingerprint: &str) -> FileImport {
    FileImport {
        item_id,
        root_id,
        rel_path: rel_path.to_owned(),
        file_name: rel_path.rsplit('/').next().unwrap().to_owned(),
        display_name: None,
        size: 10,
        modified_ms: 1,
        fingerprint: fingerprint.to_owned(),
        crc32: Some(0x1234),
        kind: BlobKind::UnityPackage,
    }
}

fn item_node(item_id: ItemId) -> FacetNode {
    FacetNode {
        item_id,
        blob_id: None,
        inner_path: None,
    }
}

fn values(names: &[&str]) -> Vec<FacetValue> {
    names
        .iter()
        .map(|name| FacetValue {
            value: (*name).to_owned(),
            target_item_id: None,
        })
        .collect()
}

fn facet_rows(store: &mut Store) -> Vec<(String, FacetOrigin, bool)> {
    store
        .load()
        .unwrap()
        .facets
        .into_iter()
        .map(|facet| (facet.value, facet.origin, facet.removed))
        .collect()
}

#[test]
fn open_creates_the_schema_and_reopening_keeps_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let mut store = Store::open(dir.path()).unwrap();
        store.create_item("Gloves").unwrap()
    };

    let mut store = Store::open(dir.path()).unwrap();
    let catalog = store.load().unwrap();

    assert_eq!(catalog.items.len(), 1);
    let item = &catalog.items[0];
    assert_eq!((item.id, item.title.as_str()), (id, "Gloves"));
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
    store.create_item("Earrings").unwrap();

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
    let id = store.create_item("gloves").unwrap();

    store
        .update_item(
            id,
            &ItemChanges {
                title: Some("Gloves".into()),
                ..Default::default()
            },
        )
        .unwrap();

    let item = store.load().unwrap().items.remove(0);
    assert_eq!(item.title, "Gloves");
    assert_eq!(item.note, "");
    assert!(item.updated_at >= item.created_at);
}

#[test]
fn add_root_is_idempotent_per_path() {
    let (_dir, mut store) = open();

    let first = store.add_root("D:/BOOTH").unwrap();
    let again = store.add_root("D:/BOOTH").unwrap();
    let other = store.add_root("E:/VRChat").unwrap();

    assert_eq!(first, again);
    assert_ne!(first, other);
    assert_eq!(store.load().unwrap().roots.len(), 2);
}

#[test]
fn add_file_reuses_the_blob_for_a_known_location_and_shares_it_across_items() {
    let (_dir, mut store) = open();
    let gloves = store.create_item("Gloves").unwrap();
    let set = store.create_item("Gloves Set").unwrap();
    let root = store.add_root("D:/BOOTH").unwrap();

    let first = store.add_file(&import(gloves, Some(root), "Gloves.unitypackage", "a")).unwrap();
    let again = store.add_file(&import(gloves, Some(root), "Gloves.unitypackage", "a")).unwrap();
    let shared = store.add_file(&import(set, Some(root), "Gloves.unitypackage", "a")).unwrap();

    assert_eq!(first, again);
    assert_eq!(first, shared);
    let catalog = store.load().unwrap();
    assert_eq!(catalog.blobs.len(), 1);
    assert_eq!(catalog.blobs[0].crc32, Some(0x1234));
    assert_eq!(catalog.locations.len(), 1);
    assert_eq!(catalog.files.len(), 2);
}

#[test]
fn the_same_relative_path_under_different_roots_or_none_is_a_different_location() {
    let (_dir, mut store) = open();
    let item = store.create_item("Gloves").unwrap();
    let booth = store.add_root("D:/BOOTH").unwrap();
    let backup = store.add_root("E:/Backup").unwrap();

    store.add_file(&import(item, Some(booth), "Gloves.unitypackage", "a")).unwrap();
    store.add_file(&import(item, Some(backup), "Gloves.unitypackage", "b")).unwrap();
    store.add_file(&import(item, None, "Gloves.unitypackage", "c")).unwrap();
    store.add_file(&import(item, None, "Gloves.unitypackage", "c")).unwrap();

    let roots: Vec<_> = store
        .load()
        .unwrap()
        .locations
        .iter()
        .map(|location| location.root_id)
        .collect();
    assert_eq!(roots, [Some(booth), Some(backup), None]);
}

#[test]
fn add_file_creates_a_new_blob_when_the_file_at_a_location_changed() {
    let (_dir, mut store) = open();
    let item = store.create_item("Gloves").unwrap();

    let old = store.add_file(&import(item, None, "D:/Gloves.unitypackage", "a")).unwrap();
    let new = store.add_file(&import(item, None, "D:/Gloves.unitypackage", "b")).unwrap();

    assert_ne!(old, new);
    let catalog = store.load().unwrap();
    assert_eq!(catalog.blobs.len(), 2);
    assert_eq!(catalog.locations.len(), 1);
    assert_eq!(catalog.locations[0].blob_id, new);
}

#[test]
fn a_blob_referenced_by_an_item_file_cannot_be_deleted() {
    let (_dir, mut store) = open();
    let item = store.create_item("Gloves").unwrap();
    let blob_id = store.add_file(&import(item, None, "D:/Gloves.unitypackage", "a")).unwrap();

    let result = diesel::delete(blob::table.find(blob_id)).execute(&mut store.conn);

    assert!(result.is_err());
}

#[test]
fn a_file_facet_must_belong_to_one_of_the_items_files() {
    let (_dir, mut store) = open();
    let gloves = store.create_item("Gloves").unwrap();
    let hair = store.create_item("Hair").unwrap();
    let blob_id = store.add_file(&import(gloves, None, "D:/Gloves.unitypackage", "a")).unwrap();
    let foreign = FacetNode {
        item_id: hair,
        blob_id: Some(blob_id),
        inner_path: None,
    };

    let result = store.replace_facets(&foreign, FacetKey::Color, FacetOrigin::Auto, &values(&["black"]));

    assert!(result.is_err());
}

#[test]
fn removing_a_file_from_an_item_drops_its_facets_and_deleting_an_item_drops_all() {
    let (_dir, mut store) = open();
    let gloves = store.create_item("Gloves").unwrap();
    let blob_id = store.add_file(&import(gloves, None, "D:/Gloves.unitypackage", "a")).unwrap();
    let file = FacetNode {
        item_id: gloves,
        blob_id: Some(blob_id),
        inner_path: Some("Gloves.unitypackage!/Assets/Black.prefab".into()),
    };
    store.replace_facets(&file, FacetKey::Color, FacetOrigin::Auto, &values(&["black"])).unwrap();
    store.replace_facets(&item_node(gloves), FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka"])).unwrap();

    diesel::delete(item_file::table.find((gloves, blob_id)))
        .execute(&mut store.conn)
        .unwrap();
    assert_eq!(facet_rows(&mut store), [("Manuka".to_owned(), FacetOrigin::Auto, false)]);

    diesel::delete(item::table.find(gloves)).execute(&mut store.conn).unwrap();
    assert!(store.load().unwrap().facets.is_empty());
}

#[test]
fn replace_facets_only_touches_the_given_origin_and_never_duplicates_a_user_value() {
    let (_dir, mut store) = open();
    let item = store.create_item("Outfit").unwrap();
    let node = item_node(item);
    store.replace_facets(&node, FacetKey::Target, FacetOrigin::User, &values(&["Kikyo"])).unwrap();

    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka", "Kikyo"]))
        .unwrap();
    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::Auto, &values(&["Karin", "Kikyo"]))
        .unwrap();

    assert_eq!(
        facet_rows(&mut store),
        [
            ("Kikyo".to_owned(), FacetOrigin::User, false),
            ("Karin".to_owned(), FacetOrigin::Auto, false),
        ]
    );
}

#[test]
fn remove_facet_replaces_any_row_for_the_value_with_a_user_removal() {
    let (_dir, mut store) = open();
    let item = store.create_item("Outfit").unwrap();
    let node = item_node(item);
    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka", "Kikyo"]))
        .unwrap();

    store.remove_facet(&node, FacetKey::Target, "Manuka").unwrap();
    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka", "Kikyo"]))
        .unwrap();

    assert_eq!(
        facet_rows(&mut store),
        [
            ("Manuka".to_owned(), FacetOrigin::User, true),
            ("Kikyo".to_owned(), FacetOrigin::Auto, false),
        ]
    );
}

#[test]
fn a_user_value_replaces_an_earlier_removal_or_auto_row_of_the_same_value() {
    let (_dir, mut store) = open();
    let item = store.create_item("Outfit").unwrap();
    let node = item_node(item);
    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka", "Kikyo"]))
        .unwrap();
    store.remove_facet(&node, FacetKey::Target, "Manuka").unwrap();

    store
        .replace_facets(&node, FacetKey::Target, FacetOrigin::User, &values(&["Manuka", "Kikyo"]))
        .unwrap();

    assert_eq!(
        facet_rows(&mut store),
        [
            ("Manuka".to_owned(), FacetOrigin::User, false),
            ("Kikyo".to_owned(), FacetOrigin::User, false),
        ]
    );
}

#[test]
fn tags_are_unique_case_insensitively_and_shared_between_items() {
    let (_dir, mut store) = open();
    let a = store.create_item("A").unwrap();
    let b = store.create_item("B").unwrap();

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
    let item = store.create_item("Hair").unwrap();
    store
        .replace_facets(&item_node(item), FacetKey::Color, FacetOrigin::Auto, &values(&["black"]))
        .unwrap();
    diesel::sql_query("UPDATE facet SET key = 'spaceship'")
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
        chain.iter().any(|message| message == "unknown FacetKey value \"spaceship\""),
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
    check(StoreKind::ALL, StoreKind::as_str);
    check(LocationState::ALL, LocationState::as_str);
    check(ImageOrigin::ALL, ImageOrigin::as_str);
    check(FacetKey::ALL, FacetKey::as_str);
    check(FacetOrigin::ALL, FacetOrigin::as_str);
    check(BlobKind::ALL, BlobKind::as_str);
}

fn write_everything(store: &mut Store) -> Result<ItemId> {
    let item = store.create_item("Gloves")?;
    let root = store.add_root("D:/BOOTH")?;
    store.add_file(&import(item, Some(root), "Gloves.unitypackage", "a"))?;
    store.replace_facets(&item_node(item), FacetKey::Target, FacetOrigin::Auto, &values(&["Manuka"]))?;
    store.tag_item(item, "gloves")?;
    let folder = store.create_folder(None, "Accessories")?;
    store.put_in_folder(item, folder)?;
    Ok(item)
}

#[test]
fn a_failed_transaction_rolls_back_every_write_inside_it() {
    let (_dir, mut store) = open();

    let result = store.transaction(|store| {
        write_everything(store)?;
        Err::<(), _>(diesel::result::Error::RollbackTransaction.into())
    });

    assert!(result.is_err());
    assert_eq!(store.load().unwrap(), Catalog::default());
}

#[test]
fn a_successful_transaction_commits_nested_writes_and_the_store_stays_usable() {
    let (_dir, mut store) = open();

    let item = store.transaction(write_everything).unwrap();
    store.create_item("Hair").unwrap();

    let catalog = store.load().unwrap();
    assert_eq!(catalog.items.len(), 2);
    assert_eq!(catalog.items[0].id, item);
    assert_eq!(catalog.files.len(), 1);
    assert_eq!(catalog.facets.len(), 1);
    assert_eq!(catalog.roots.len(), 1);
    assert_eq!(catalog.item_tags.len(), 1);
    assert_eq!(catalog.item_folders.len(), 1);
}

#[test]
fn moving_and_marking_a_location_keeps_its_blob() {
    let (_dir, mut store) = open();
    let item = store.create_item("Gloves").unwrap();
    let root = store.add_root("D:/BOOTH").unwrap();
    let blob_id = store.add_file(&import(item, Some(root), "Gloves.zip", "a")).unwrap();
    let location = store.load().unwrap().locations[0].id;

    store.mark_missing(location).unwrap();
    assert_eq!(store.load().unwrap().locations[0].state, LocationState::Missing);
    store.move_location(location, "Accessories/Gloves.zip", 5).unwrap();

    let moved = store.load().unwrap().locations.remove(0);
    assert_eq!((moved.blob_id, moved.rel_path.as_str(), moved.modified_ms, moved.state), (blob_id, "Accessories/Gloves.zip", 5, LocationState::Present));
}

#[test]
fn removing_an_extracted_copy_drops_its_blob_but_keeps_a_shared_or_referenced_one() {
    let (_dir, mut store) = open();
    let item = store.create_item("Gloves").unwrap();
    let other = store.create_item("Set").unwrap();
    let zip = store.add_file(&import(item, None, "D:/Gloves.zip", "z")).unwrap();
    let copy = store.add_file(&import(item, None, "D:/Gloves/Gloves.unitypackage", "c")).unwrap();
    let shared = store.add_file(&import(item, None, "D:/Gloves/readme.txt", "r")).unwrap();
    store.add_file(&import(other, None, "D:/Gloves/readme.txt", "r")).unwrap();
    store.set_extracted_from(item, copy, Some((zip, "Gloves.unitypackage"))).unwrap();
    let catalog = store.load().unwrap();
    let file = catalog.files.iter().find(|file| file.blob_id == copy).unwrap();
    assert_eq!((file.extracted_from_blob_id, file.extracted_from_path.as_deref()), (Some(zip), Some("Gloves.unitypackage")));

    store.remove_item_file(item, copy).unwrap();
    store.remove_item_file(item, shared).unwrap();

    let catalog = store.load().unwrap();
    let blobs: Vec<_> = catalog.blobs.iter().map(|blob| blob.id).collect();
    assert_eq!(blobs, [zip, shared]);
    assert_eq!(catalog.locations.len(), 2);
}
