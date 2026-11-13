use std::fs;

use super::*;

#[test]
fn inspect_file_reads_size_name_and_kind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Outfit.unitypackage");
    fs::write(&path, [0x1f, 0x8b, 0x08, 0x00, 0x00]).unwrap();

    let info = inspect_file(&path).unwrap();

    assert_eq!(info.name, "Outfit.unitypackage");
    assert_eq!(info.size, 5);
    assert_eq!(info.kind, FileKind::UnityPackage);
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
fn insert_skips_paths_already_in_the_library() {
    let file = |path: &str| FileInfo {
        path: PathBuf::from(path),
        name: path.to_owned(),
        size: 1,
        kind: FileKind::Other,
    };
    let mut library = Library::default();

    assert_eq!(library.insert([file("a.zip"), file("b.zip"), file("a.zip")]), 2);
    assert_eq!(library.insert([file("b.zip"), file("c.zip")]), 1);

    let names: Vec<_> = library.assets().iter().map(|asset| asset.file.name.as_str()).collect();
    assert_eq!(names, ["a.zip", "b.zip", "c.zip"]);
}

fn sample() -> (Library, Vec<AssetId>) {
    let file = |name: &str, size: u64, kind: FileKind| FileInfo {
        path: PathBuf::from(name),
        name: name.to_owned(),
        size,
        kind,
    };
    let mut library = Library::default();
    library.insert([
        file("Outfit.unitypackage", 30, FileKind::UnityPackage),
        file("hair.zip", 50, FileKind::Archive),
        file("cover.png", 10, FileKind::Image),
    ]);
    let ids = library.assets().iter().map(|asset| asset.id).collect();
    (library, ids)
}

fn names<'a>(assets: &[&'a Asset]) -> Vec<&'a str> {
    assets.iter().map(|asset| asset.file.name.as_str()).collect()
}

#[test]
fn query_sorts_newest_first_by_name_or_largest_first() {
    let (library, _) = sample();

    assert_eq!(
        names(&library.query(&Scope::All, "", SortKey::Added)),
        ["cover.png", "hair.zip", "Outfit.unitypackage"]
    );
    assert_eq!(
        names(&library.query(&Scope::All, "", SortKey::Name)),
        ["cover.png", "hair.zip", "Outfit.unitypackage"]
    );
    assert_eq!(
        names(&library.query(&Scope::All, "", SortKey::Size)),
        ["hair.zip", "Outfit.unitypackage", "cover.png"]
    );
}

#[test]
fn query_combines_scope_with_search_over_names_and_tags() {
    let (mut library, ids) = sample();
    library.add_tag(ids[1], "Kikyo");
    library.add_tag(ids[0], "kikyo-outfit");
    library.move_to_folder(ids[0], Some("Avatars"));

    let query = |scope: Scope, search: &str| names(&library.query(&scope, search, SortKey::Name));
    assert_eq!(query(Scope::All, " KIKYO "), ["hair.zip", "Outfit.unitypackage"]);
    assert_eq!(query(Scope::Kind(FileKind::Archive), "kikyo"), ["hair.zip"]);
    assert_eq!(query(Scope::Tag("Kikyo".into()), ""), ["hair.zip"]);
    assert_eq!(query(Scope::Untagged, ""), ["cover.png"]);
    assert_eq!(query(Scope::Folder("Avatars".into()), ""), ["Outfit.unitypackage"]);
    assert_eq!(library.count(&Scope::Unfiled), 2);
}

#[test]
fn tags_are_trimmed_deduplicated_and_counted() {
    let (mut library, ids) = sample();

    assert!(library.add_tag(ids[0], " outfit "));
    assert!(!library.add_tag(ids[0], "outfit"));
    assert!(!library.add_tag(ids[0], "   "));
    assert!(library.add_tag(ids[1], "outfit"));
    assert!(library.remove_tag(ids[1], "outfit"));
    assert!(library.add_tag(ids[2], "cover"));

    assert_eq!(library.tags().into_iter().collect::<Vec<_>>(), [("cover", 1), ("outfit", 1)]);
}

#[test]
fn moving_to_a_folder_creates_it_and_keeps_it_after_emptying() {
    let (mut library, ids) = sample();

    assert!(library.create_folder("Shaders"));
    assert!(!library.create_folder(" Shaders "));
    assert!(library.move_to_folder(ids[0], Some(" Avatars ")));
    assert!(library.move_to_folder(ids[0], None));

    assert_eq!(library.folders().collect::<Vec<_>>(), ["Avatars", "Shaders"]);
    assert_eq!(library.get(ids[0]).unwrap().folder, None);
}
