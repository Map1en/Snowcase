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
