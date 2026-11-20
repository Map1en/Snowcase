use std::fs;

use snowcase_store::LocationState;

use super::*;

fn file(name: &str, size: u64, kind: FileKind) -> FileInfo {
    FileInfo {
        path: format!("D:/BOOTH/{name}"),
        name: name.to_owned(),
        size,
        modified_ms: 1,
        kind,
    }
}

fn sample() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    library
        .import(vec![
            file("Gloves.unitypackage", 30, FileKind::UnityPackage),
            file("Hair.zip", 50, FileKind::Archive),
            file("cover.png", 10, FileKind::Image),
        ])
        .unwrap();
    (dir, library)
}

fn id_of(library: &Library, title: &str) -> ItemId {
    library.items().iter().find(|item| item.title == title).unwrap().id
}

fn titles<'a>(items: &[&'a Item]) -> Vec<&'a str> {
    items.iter().map(|item| item.title.as_str()).collect()
}

#[test]
fn inspect_file_reads_size_name_kind_and_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Outfit.unitypackage");
    fs::write(&path, [0x1f, 0x8b, 0x08, 0x00, 0x00]).unwrap();

    let info = inspect_file(&path).unwrap();

    assert_eq!(info.name, "Outfit.unitypackage");
    assert_eq!(info.size, 5);
    assert_eq!(info.kind, FileKind::UnityPackage);
    assert!(info.modified_ms > 0);
}

#[test]
fn inspect_file_rejects_directories_and_missing_paths() {
    let dir = tempfile::tempdir().unwrap();

    assert!(matches!(inspect_file(dir.path()), Err(InspectError::NotAFile(_))));
    assert!(matches!(
        inspect_file(&dir.path().join("missing.zip")),
        Err(InspectError::Io { .. })
    ));
}

#[test]
fn collect_files_walks_folders_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("Gloves");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("b.png"), b"").unwrap();
    fs::write(nested.join("a.unitypackage"), b"").unwrap();
    let loose = dir.path().join("hair.zip");
    fs::write(&loose, b"").unwrap();

    let files = collect_files(&[loose.clone(), dir.path().join("Gloves")]);

    assert_eq!(files, [loose, nested.join("a.unitypackage"), nested.join("b.png")]);
}

#[test]
fn import_creates_one_item_per_new_file_and_persists_it() {
    let (dir, mut library) = sample();

    let again = library
        .import(vec![
            file("Hair.zip", 50, FileKind::Archive),
            file("Boots.fbx", 5, FileKind::Model),
            file("Boots.fbx", 5, FileKind::Model),
        ])
        .unwrap();
    assert_eq!(again, 1);
    drop(library);

    let library = Library::open(dir.path()).unwrap();
    assert_eq!(library.count(&Scope::All), 4);
    let gloves = id_of(&library, "Gloves");
    let files = library.files(gloves);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file.file_name, "Gloves.unitypackage");
    assert_eq!(files[0].file.extracted_from_blob_id, None);
    assert_eq!(files[0].blob.kind, BlobKind::UnityPackage);
    assert_eq!(library.location_path(files[0].locations[0]), "D:/BOOTH/Gloves.unitypackage");
    assert_eq!(files[0].locations[0].state, LocationState::Present);
}

#[test]
fn parent_node_climbs_folders_then_containers_then_the_file_and_the_item() {
    let item = ItemId(1);
    let at = |path: &str| FacetNode {
        item_id: item,
        blob_id: Some(snowcase_store::BlobId(2)),
        inner_path: Some(path.to_owned()),
    };
    let mut node = at("Manuka/Outfit.unitypackage!/Assets/Shop/Black.prefab");
    let mut chain = Vec::new();
    while let Some(parent) = parent_node(&node) {
        chain.push(parent.inner_path.clone().unwrap_or_else(|| {
            if parent.blob_id.is_some() { "<file>" } else { "<item>" }.to_owned()
        }));
        node = parent;
    }

    assert_eq!(
        chain,
        [
            "Manuka/Outfit.unitypackage!/Assets/Shop",
            "Manuka/Outfit.unitypackage!/Assets",
            "Manuka/Outfit.unitypackage",
            "Manuka",
            "<file>",
            "<item>",
        ]
    );
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

fn shown(library: &Library, node: &FacetNode, key: FacetKey) -> Vec<(String, FacetOrigin)> {
    library
        .facets(node, key)
        .into_iter()
        .map(|facet| (facet.value, facet.origin))
        .collect()
}

fn outfit() -> (tempfile::TempDir, Library, FacetNode, FacetNode, FacetNode) {
    let (dir, library) = sample();
    let gloves = id_of(&library, "Gloves");
    let blob_id = library.files(gloves)[0].blob.id;
    let item = item_node(gloves);
    let file = FacetNode {
        item_id: gloves,
        blob_id: Some(blob_id),
        inner_path: None,
    };
    let prefab = FacetNode {
        inner_path: Some("Gloves.unitypackage!/Assets/Shop/Black.prefab".into()),
        ..file.clone()
    };
    (dir, library, item, file, prefab)
}

#[test]
fn facets_inherit_down_the_tree_and_the_nearest_node_wins_as_a_whole() {
    let (_dir, mut library, item, file, prefab) = outfit();
    library.set_auto_facets(&item, FacetKey::Target, &values(&["Manuka", "Kikyo"])).unwrap();

    let inherited = library.facets(&prefab, FacetKey::Target);
    assert_eq!(inherited.len(), 2);
    assert_eq!(inherited[0].from, item);

    library.set_auto_facets(&file, FacetKey::Target, &values(&["Karin"])).unwrap();
    assert_eq!(shown(&library, &prefab, FacetKey::Target), [("Karin".into(), FacetOrigin::Auto)]);
    assert_eq!(library.facets(&prefab, FacetKey::Target)[0].from, file);
    assert_eq!(shown(&library, &item, FacetKey::Target).len(), 2);
    assert!(library.facets(&prefab, FacetKey::Color).is_empty());
}

#[test]
fn recognising_again_never_overrides_a_user_value() {
    let (_dir, mut library, item, _file, prefab) = outfit();
    library.set_auto_facets(&item, FacetKey::Color, &values(&["black"])).unwrap();
    library.set_user_facets(&item, FacetKey::Color, &values(&["navy"])).unwrap();

    library.set_auto_facets(&item, FacetKey::Color, &values(&["white"])).unwrap();

    assert_eq!(shown(&library, &prefab, FacetKey::Color), [("navy".into(), FacetOrigin::User)]);
}

#[test]
fn a_removed_value_does_not_come_back_when_recognising_again() {
    let (_dir, mut library, item, file, _prefab) = outfit();
    library.set_auto_facets(&item, FacetKey::Target, &values(&["Manuka", "Kikyo"])).unwrap();

    library.remove_facet(&item, FacetKey::Target, "Manuka").unwrap();
    library.set_auto_facets(&item, FacetKey::Target, &values(&["Manuka", "Kikyo"])).unwrap();

    assert_eq!(shown(&library, &file, FacetKey::Target), [("Kikyo".into(), FacetOrigin::Auto)]);
}

#[test]
fn removing_an_inherited_value_on_a_child_hides_it_only_below_that_child() {
    let (_dir, mut library, item, file, prefab) = outfit();
    library.set_auto_facets(&item, FacetKey::Target, &values(&["Manuka", "Kikyo"])).unwrap();

    library.remove_facet(&file, FacetKey::Target, "Manuka").unwrap();

    assert_eq!(shown(&library, &prefab, FacetKey::Target), [("Kikyo".into(), FacetOrigin::Auto)]);
    assert_eq!(shown(&library, &item, FacetKey::Target).len(), 2);
}

#[test]
#[should_panic(expected = "takes a single value")]
fn a_single_valued_key_rejects_several_values() {
    let (_dir, mut library, item, _file, _prefab) = outfit();
    library.set_auto_facets(&item, FacetKey::Color, &values(&["black", "white"])).unwrap();
}

#[test]
fn a_new_library_starts_with_one_default_folder_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    {
        let library = Library::open(dir.path()).unwrap();
        let names: Vec<_> = library.folder_tree().iter().map(|row| row.folder.name.clone()).collect();
        assert_eq!(names, [DEFAULT_FOLDER]);
        assert!(library.items().is_empty());
    }

    let library = Library::open(dir.path()).unwrap();
    assert_eq!(library.folder_tree().len(), 1);
}

#[test]
fn query_sorts_by_newest_name_or_size() {
    let (_dir, library) = sample();

    assert_eq!(
        titles(&library.query(&Scope::All, "", SortKey::Added)),
        ["cover", "Hair", "Gloves"]
    );
    assert_eq!(
        titles(&library.query(&Scope::All, "", SortKey::Name)),
        ["cover", "Gloves", "Hair"]
    );
    assert_eq!(
        titles(&library.query(&Scope::All, "", SortKey::Size)),
        ["Hair", "Gloves", "cover"]
    );
}

#[test]
fn query_combines_scope_with_search_over_titles_tags_and_file_names() {
    let (_dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");
    let kikyo = library.tag_item(hair, " Kikyo ").unwrap().unwrap();
    let folder = library.create_folder(None, "Avatars").unwrap().unwrap();
    library.put_in_folder(gloves, folder).unwrap();

    let query = |scope: Scope, search: &str| titles(&library.query(&scope, search, SortKey::Name));
    assert_eq!(query(Scope::All, " KIKYO "), ["Hair"]);
    assert_eq!(query(Scope::All, ".unitypackage"), ["Gloves"]);
    assert_eq!(query(Scope::Tag(kikyo), ""), ["Hair"]);
    assert_eq!(query(Scope::Folder(folder), ""), ["Gloves"]);
}

#[test]
fn blank_tags_and_folders_are_ignored_and_sibling_folders_are_unique() {
    let (_dir, mut library) = sample();
    let hair = id_of(&library, "Hair");

    assert_eq!(library.tag_item(hair, "   ").unwrap(), None);
    assert_eq!(library.create_folder(None, " ").unwrap(), None);
    let avatars = library.create_folder(None, "Avatars").unwrap();
    assert_eq!(library.create_folder(None, " avatars ").unwrap(), avatars);
    let kikyo = library.create_folder(avatars, "Kikyo").unwrap();
    library.create_folder(None, "Shaders").unwrap();

    let tree: Vec<_> = library
        .folder_tree()
        .iter()
        .map(|row| (row.folder.name.as_str(), row.depth))
        .collect();
    assert_eq!(tree, [("Default", 0), ("Avatars", 0), ("Kikyo", 1), ("Shaders", 0)]);
    assert_ne!(kikyo, avatars);
}

#[test]
fn tag_counts_skip_tags_no_item_uses_any_more() {
    let (_dir, mut library) = sample();
    let hair = id_of(&library, "Hair");
    let gloves = id_of(&library, "Gloves");
    let outfit = library.tag_item(hair, "outfit").unwrap().unwrap();
    library.tag_item(gloves, "outfit").unwrap();

    library.untag_item(hair, outfit).unwrap();
    assert_eq!(library.tag_counts().len(), 1);
    library.untag_item(gloves, outfit).unwrap();

    assert!(library.tag_counts().is_empty());
}

fn assert_matches_a_full_reload(library: &Library, dir: &std::path::Path) {
    let incremental = library.snapshot();
    let reloaded = Library::open(dir).unwrap().snapshot();
    assert_eq!(incremental.catalog, reloaded.catalog);
    assert_eq!(incremental.facts, reloaded.facts);
}

#[test]
fn every_write_keeps_the_snapshot_equal_to_a_full_reload() {
    let (dir, mut library) = sample();
    assert_matches_a_full_reload(&library, dir.path());
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");

    let outfit = library.tag_item(gloves, "outfit").unwrap().unwrap();
    library.tag_item(hair, "outfit").unwrap();
    assert_matches_a_full_reload(&library, dir.path());
    library.untag_item(gloves, outfit).unwrap();
    let folder = library.create_folder(None, "Avatars").unwrap().unwrap();
    library.put_in_folder(hair, folder).unwrap();
    library.put_in_folder(gloves, folder).unwrap();
    library.remove_from_folder(gloves, folder).unwrap();
    library
        .update_item(
            hair,
            &ItemChanges {
                title: Some("Long Hair".into()),
                ..Default::default()
            },
        )
        .unwrap();
    library
        .set_auto_facets(&item_node(hair), FacetKey::Target, &values(&["Manuka", "Kikyo"]))
        .unwrap();
    library.remove_facet(&item_node(hair), FacetKey::Target, "Kikyo").unwrap();
    library
        .import(vec![
            file("Boots.fbx", 5, FileKind::Model),
            file("Hair.zip", 50, FileKind::Archive),
        ])
        .unwrap();

    assert_matches_a_full_reload(&library, dir.path());
}
