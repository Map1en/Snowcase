use std::fs;
use std::path::Path;

use super::*;

fn touch(root: &Path, rel_path: &str, bytes: &[u8]) {
    let path = root.join(rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn library() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, "Outfit_v1.0.zip", b"PK");
    touch(root, "Outfit_v1.0/Outfit.unitypackage", b"pkg");
    touch(root, "Outfit_v1.0/Textures/Pink/body.png", b"png");
    touch(root, "bh01boost.unitypackage/bh01boost.unitypackage", b"pkg");
    touch(root, "bh01boost.unitypackage/__MACOSX/._bh01boost.unitypackage", b"x");
    touch(root, "._cover.png", b"x");
    touch(root, "Mafuyu/Assets/Shop/Outfit.prefab", b"prefab");
    touch(root, "Mafuyu/ProjectSettings/ProjectVersion.txt", b"m_EditorVersion: 2022.3.22f1");
    touch(root, "Mafuyu/Library/ArtifactDB", b"cache");
    touch(root, "Gesture-Manager-master/Editor/Manager.cs.meta", b"meta");
    touch(root, "VRCX_noty.psd", b"8BPS");
    dir
}

fn paths(walk: &Walk) -> Vec<&str> {
    walk.files.iter().map(|file| file.rel_path.as_str()).collect()
}

#[test]
fn walk_lists_a_folders_files_before_its_subfolders_in_name_order() {
    let dir = library();

    let walk = walk(dir.path());

    assert_eq!(
        paths(&walk),
        [
            "Outfit_v1.0.zip",
            "VRCX_noty.psd",
            "Gesture-Manager-master/Editor/Manager.cs.meta",
            "Outfit_v1.0/Outfit.unitypackage",
            "Outfit_v1.0/Textures/Pink/body.png",
            "bh01boost.unitypackage/bh01boost.unitypackage",
        ]
    );
    let kinds: Vec<_> = walk.files.iter().map(|file| file.kind).collect();
    assert_eq!(
        kinds,
        [
            FileKind::Archive,
            FileKind::Image,
            FileKind::Other,
            FileKind::UnityPackage,
            FileKind::Image,
            FileKind::UnityPackage,
        ]
    );
    assert_eq!(walk.files[0].size, 2);
    assert!(walk.files.iter().all(|file| file.modified_ms > 0));
    assert!(walk.errors.is_empty());
}

#[test]
fn walk_records_unity_projects_without_entering_them() {
    let dir = library();

    let walk = walk(dir.path());

    assert_eq!(walk.unity_projects, ["Mafuyu"]);
    assert!(paths(&walk).iter().all(|path| !path.starts_with("Mafuyu")));
}

#[test]
fn a_folder_with_assets_but_no_project_settings_is_not_a_unity_project() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "Package/Assets/Shop/Outfit.prefab", b"prefab");

    let walk = walk(dir.path());

    assert!(walk.unity_projects.is_empty());
    assert_eq!(paths(&walk), ["Package/Assets/Shop/Outfit.prefab"]);
}

#[test]
fn a_root_that_is_itself_a_unity_project_lists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "Assets/Outfit.prefab", b"prefab");
    touch(dir.path(), "ProjectSettings/ProjectVersion.txt", b"version");

    let walk = walk(dir.path());

    assert_eq!(walk.unity_projects, [""]);
    assert!(walk.files.is_empty());
}

#[test]
fn a_missing_root_is_reported_as_an_error() {
    let dir = tempfile::tempdir().unwrap();

    let walk = walk(&dir.path().join("gone"));

    assert!(walk.files.is_empty());
    assert_eq!(walk.errors.len(), 1);
    assert_eq!(walk.errors[0].rel_path, "");
}

fn found(rel_path: &str, size: u64, modified_ms: i64) -> FoundFile {
    FoundFile {
        rel_path: rel_path.into(),
        size,
        modified_ms,
        kind: FileKind::Archive,
    }
}

fn known(rel_path: &str, size: u64, modified_ms: i64) -> KnownFile {
    KnownFile {
        rel_path: rel_path.into(),
        size,
        modified_ms,
    }
}

#[test]
fn diff_sorts_files_into_new_changed_unchanged_and_missing() {
    let before = [
        known("Same.zip", 10, 1),
        known("Resized.zip", 10, 1),
        known("Touched.zip", 10, 1),
        known("Gone/B.zip", 10, 1),
        known("Gone/A.zip", 10, 1),
    ];
    let seen = [
        found("Same.zip", 10, 1),
        found("Resized.zip", 11, 1),
        found("Touched.zip", 10, 2),
        found("New.zip", 5, 1),
    ];

    let changes = diff(&before, &seen);

    assert_eq!(changes.unchanged, 1);
    let changed: Vec<_> = changes.changed.iter().map(|file| file.rel_path.as_str()).collect();
    assert_eq!(changed, ["Resized.zip", "Touched.zip"]);
    assert_eq!(changes.new, [found("New.zip", 5, 1)]);
    assert_eq!(changes.missing, ["Gone/A.zip", "Gone/B.zip"]);
}

#[test]
fn diffing_a_walk_against_itself_changes_nothing() {
    let dir = library();
    let first = walk(dir.path());
    let before: Vec<_> = first
        .files
        .iter()
        .map(|file| known(&file.rel_path, file.size, file.modified_ms))
        .collect();

    let changes = diff(&before, &walk(dir.path()).files);

    assert_eq!(changes, Changes { unchanged: first.files.len(), ..Changes::default() });
}
