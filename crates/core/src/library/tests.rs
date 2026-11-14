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
fn import_creates_one_inbox_item_per_new_file_and_persists_it() {
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
    assert_eq!(library.count(&Scope::Inbox), 4);
    let gloves = id_of(&library, "Gloves");
    let versions = library.versions(gloves);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version.label, None);
    let files = &versions[0].files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file.file_name, "Gloves.unitypackage");
    assert_eq!(files[0].file.role, Some(FileRole::Package));
    assert_eq!(files[0].locations[0].path, "D:/BOOTH/Gloves.unitypackage");
    assert_eq!(files[0].locations[0].state, LocationState::Present);
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
    library
        .update_item(
            gloves,
            &ItemChanges {
                status: Some(ItemStatus::Confirmed),
                ..Default::default()
            },
        )
        .unwrap();

    let query = |scope: Scope, search: &str| titles(&library.query(&scope, search, SortKey::Name));
    assert_eq!(query(Scope::All, " KIKYO "), ["Hair"]);
    assert_eq!(query(Scope::All, ".unitypackage"), ["Gloves"]);
    assert_eq!(query(Scope::Tag(kikyo), ""), ["Hair"]);
    assert_eq!(query(Scope::Folder(folder), ""), ["Gloves"]);
    assert_eq!(query(Scope::Inbox, ""), ["cover", "Hair"]);
    assert_eq!(query(Scope::InLibrary, ""), ["Gloves"]);
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
