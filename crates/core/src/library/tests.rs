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
        titles(&library.query(&Filter::default(), SortKey::Added)),
        ["cover", "Hair", "Gloves"]
    );
    assert_eq!(
        titles(&library.query(&Filter::default(), SortKey::Name)),
        ["cover", "Gloves", "Hair"]
    );
    assert_eq!(
        titles(&library.query(&Filter::default(), SortKey::Size)),
        ["Hair", "Gloves", "cover"]
    );
}

#[test]
fn query_combines_scope_with_search_over_titles_tags_and_file_names() {
    let (_dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");
    let kikyo = library.tag(&item_node(hair), " Kikyo ").unwrap().unwrap();
    let folder = library.create_folder(None, "Avatars").unwrap().unwrap();
    library.put_in_folder(gloves, folder).unwrap();

    let query = |filter: Filter| titles(&library.query(&filter, SortKey::Name));
    assert_eq!(query(Filter::search(" KIKYO ")), ["Hair"]);
    assert_eq!(query(Filter::search(".unitypackage")), ["Gloves"]);
    assert_eq!(query(Filter { tag: Some(kikyo), ..Filter::default() }), ["Hair"]);
    assert_eq!(query(Filter::scope(Scope::Folder(folder))), ["Gloves"]);
}

#[test]
fn blank_tags_and_folders_are_ignored_and_sibling_folders_are_unique() {
    let (_dir, mut library) = sample();
    let hair = id_of(&library, "Hair");

    assert_eq!(library.tag(&item_node(hair), "   ").unwrap(), None);
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
    let outfit = library.tag(&item_node(hair), "outfit").unwrap().unwrap();
    library.tag(&item_node(gloves), "outfit").unwrap();

    library.untag(&item_node(hair), outfit).unwrap();
    assert_eq!(library.tag_counts().len(), 1);
    library.untag(&item_node(gloves), outfit).unwrap();

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

    let outfit = library.tag(&item_node(gloves), "outfit").unwrap().unwrap();
    library.tag(&item_node(hair), "outfit").unwrap();
    assert_matches_a_full_reload(&library, dir.path());
    library.untag(&item_node(gloves), outfit).unwrap();
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

#[test]
fn a_tag_on_a_file_finds_its_item_and_spellings_share_one_tag() {
    let (_dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let blob_id = library.files(gloves)[0].blob.id;
    let file = FacetNode {
        item_id: gloves,
        blob_id: Some(blob_id),
        inner_path: None,
    };

    let olive = library.tag(&file, "Olive Branch").unwrap().unwrap();
    let again = library.tag(&item_node(gloves), " ＯＬＩＶＥ branch ").unwrap().unwrap();

    assert_eq!(olive, again);
    assert_eq!(library.snapshot().catalog.tags[0].name, "Olive Branch");
    let tagged = Filter { tag: Some(olive), ..Filter::default() };
    assert_eq!(titles(&library.query(&tagged, SortKey::Name)), ["Gloves"]);
    assert_eq!(titles(&library.query(&Filter::search("olive"), SortKey::Name)), ["Gloves"]);
    assert_eq!(
        library.hit(gloves, "oliv"),
        Some(Hit::Tag { name: "Olive Branch".into(), file: Some("Gloves.unitypackage".into()) })
    );
    library.untag(&item_node(gloves), olive).unwrap();
    assert!(library.tags_of(gloves).is_empty());
    assert_eq!(titles(&library.query(&tagged, SortKey::Name)), ["Gloves"]);
}

fn facet(value: &str) -> FacetValue {
    FacetValue {
        value: value.into(),
        target_item_id: None,
    }
}

#[test]
fn attribute_filters_narrow_by_category_and_target_and_list_their_values() {
    let (_dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");
    library.set_auto_facets(&item_node(gloves), FacetKey::Category, &[facet("Outfit")]).unwrap();
    library.set_auto_facets(&item_node(hair), FacetKey::Category, &[facet("Hair")]).unwrap();
    library
        .set_auto_facets(&item_node(gloves), FacetKey::Target, &[facet("Mafuyu"), facet("Cian")])
        .unwrap();
    library.set_auto_facets(&item_node(hair), FacetKey::Target, &[facet("Mafuyu")]).unwrap();

    let query = |filter: Filter| titles(&library.query(&filter, SortKey::Name));
    assert_eq!(query(Filter { category: Some("Hair".into()), ..Filter::default() }), ["Hair"]);
    assert_eq!(query(Filter { target: Some("Mafuyu".into()), ..Filter::default() }), ["Gloves", "Hair"]);
    assert_eq!(query(Filter { target: Some("Cian".into()), ..Filter::default() }), ["Gloves"]);
    assert_eq!(library.values(FacetKey::Target), [("Mafuyu".to_owned(), 2), ("Cian".to_owned(), 1)]);
    assert_eq!(library.hit(gloves, "cia"), Some(Hit::Attribute("Cian".into())));

    library.remove_facet(&item_node(gloves), FacetKey::Target, "Cian").unwrap();
    let cian = Filter { target: Some("Cian".into()), ..Filter::default() };
    assert!(library.query(&cian, SortKey::Name).is_empty());
}

#[test]
fn hints_name_other_versions_of_the_same_title() {
    let (_dir, mut library) = sample();
    library
        .import(vec![FileInfo {
            path: "D:/BOOTH/old/Gloves.unitypackage".into(),
            ..file("Gloves.unitypackage", 31, FileKind::UnityPackage)
        }])
        .unwrap();
    let ids: Vec<ItemId> = library.items().iter().filter(|item| item.title == "Gloves").map(|item| item.id).collect();
    library.set_auto_facets(&item_node(ids[0]), FacetKey::Version, &[facet("1.0")]).unwrap();
    library.set_auto_facets(&item_node(ids[1]), FacetKey::Version, &[facet("1.1")]).unwrap();

    assert_eq!(library.hints(ids[0]).versions, [(ids[1], "1.1".to_owned())]);
    assert_eq!(library.hints(ids[1]).versions, [(ids[0], "1.0".to_owned())]);
    assert!(library.hints(id_of(&library, "Hair")).versions.is_empty());
}

#[test]
fn tag_suggestions_offer_close_spellings_most_used_first() {
    let (_dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");
    library.tag(&item_node(gloves), "Black Tights").unwrap();
    library.tag(&item_node(hair), "Black Tights").unwrap();
    library.tag(&item_node(hair), "Black").unwrap();
    library.tag(&item_node(hair), "Olive").unwrap();

    let names = |draft: &str| -> Vec<String> {
        library.suggest_tags(draft).iter().map(|(tag, _)| tag.name.clone()).collect()
    };
    assert_eq!(names("black tights long"), ["Black Tights", "Black"]);
    assert_eq!(names("ＢＬＡＣＫ"), ["Black Tights"]);
    assert_eq!(names("o"), Vec::<String>::new());
    assert_eq!(names(""), ["Black Tights", "Black", "Olive"]);
}

#[test]
fn merging_moves_files_tags_folders_and_user_values_and_drops_the_other_item() {
    let (dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let hair = id_of(&library, "Hair");
    let hair_blob = library.files(hair)[0].blob.id;
    let hair_file = FacetNode {
        item_id: hair,
        blob_id: Some(hair_blob),
        inner_path: None,
    };
    let value = |text: &str| FacetValue {
        value: text.into(),
        target_item_id: None,
    };
    library.tag(&item_node(hair), "winter").unwrap();
    library.tag(&hair_file, "olive").unwrap();
    let folder = library.create_folder(None, "Avatars").unwrap().unwrap();
    library.put_in_folder(hair, folder).unwrap();
    library.set_user_facets(&item_node(hair), FacetKey::Category, &[value("hair")]).unwrap();
    library.set_auto_facets(&item_node(hair), FacetKey::Shop, &[value("Old Shop")]).unwrap();
    library.set_auto_facets(&item_node(gloves), FacetKey::Shop, &[value("Shop")]).unwrap();

    library.merge_items(gloves, hair).unwrap();

    assert!(library.item(hair).is_none());
    assert_eq!(library.files(gloves).len(), 2);
    assert!(library.tags_of(gloves).iter().any(|tag| tag.name == "winter"));
    assert_eq!(library.file_tags(gloves, hair_blob)[0].name, "olive");
    assert_eq!(library.folders_of(gloves)[0].name, "Avatars");
    let values = |key| -> Vec<String> { library.facets(&item_node(gloves), key).into_iter().map(|facet| facet.value).collect() };
    assert_eq!(values(FacetKey::Category), ["hair"]);
    assert_eq!(values(FacetKey::Shop), ["Shop"], "the other item's automatic values are dropped");
    assert_matches_a_full_reload(&library, dir.path());
}

#[test]
fn linking_a_booth_page_sets_title_shop_link_and_a_stored_cover() {
    let (dir, mut library) = sample();
    let gloves = id_of(&library, "Gloves");
    let booth = crate::booth::BoothItem {
        id: 7,
        name: "Sample Gloves".into(),
        shop: "Sample Shop".into(),
        url: "https://shop.booth.pm/items/7".into(),
        image: Some("https://booth.pximg.net/a/1.jpg".into()),
    };
    let picture = crate::booth::Picture {
        bytes: b"jpeg bytes".to_vec(),
        mime: "image/jpeg".into(),
    };

    library.link_booth(gloves, &booth, Some(&picture)).unwrap();
    library.link_booth(gloves, &booth, Some(&picture)).unwrap();

    assert_eq!(library.item(gloves).unwrap().title, "Sample Gloves");
    assert_eq!(library.facts(gloves).unwrap().shop.as_deref(), Some("Sample Shop"));
    assert_eq!(library.booth_url(gloves), Some("https://shop.booth.pm/items/7"));
    assert_eq!(library.snapshot().catalog.sources.len(), 1, "linking again replaces the link");
    let cover = library.cover_path(gloves).unwrap();
    assert!(cover.starts_with(dir.path().join("media")));
    assert_eq!(std::fs::read(&cover).unwrap(), b"jpeg bytes");
    assert_matches_a_full_reload(&library, dir.path());
}
