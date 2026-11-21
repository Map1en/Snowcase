use std::fs;
use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use zip::write::SimpleFileOptions;

use super::*;
use crate::names::CodePage;

const PACKAGE: &[u8] = b"package bytes";

fn write(root: &Path, rel_path: &str, bytes: &[u8]) {
    let path = root.join(rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn write_zip(root: &Path, rel_path: &str, members: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(root.join(rel_path)).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in members {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn scanned(copy: &[u8]) -> (tempfile::TempDir, tempfile::TempDir, Library) {
    let data = tempfile::tempdir().unwrap();
    let assets = tempfile::tempdir().unwrap();
    write_zip(assets.path(), "Outfit.zip", &[("Outfit/Outfit.unitypackage", PACKAGE)]);
    write(assets.path(), "Outfit/Outfit/Outfit.unitypackage", copy);
    let mut library = Library::open(data.path()).unwrap();
    library.scan_root(assets.path(), CodePage::Other).unwrap();
    (data, assets, library)
}

fn tasks(library: &Library) -> Vec<(String, Task)> {
    library
        .pending_work()
        .into_iter()
        .map(|work| (work.path.file_name().unwrap().to_string_lossy().into_owned(), work.task))
        .collect()
}

fn run_all(library: &mut Library) -> Vec<Outcome> {
    let outcomes: Vec<Outcome> = library.pending_work().into_iter().map(run).collect();
    library.apply_work(&outcomes).unwrap();
    outcomes
}

fn copy_is_extracted(library: &Library) -> bool {
    let item = library.items()[0].id;
    library
        .files(item)
        .iter()
        .any(|view| view.file.file_name == "Outfit.unitypackage" && view.file.extracted_from_blob_id.is_some())
}

#[test]
fn a_scan_leaves_fingerprints_for_every_file_and_a_crc_for_each_extracted_copy() {
    let (_data, _assets, library) = scanned(PACKAGE);

    let mut found = tasks(&library);
    found.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(
        found,
        [
            ("Outfit.unitypackage".into(), Task::Crc { expected: crc32fast::hash(PACKAGE) }),
            ("Outfit.unitypackage".into(), Task::Package { member: None, bytes: PACKAGE.len() as u64 }),
            ("Outfit.zip".into(), Task::Fingerprint),
        ]
    );
}

#[test]
fn applied_results_are_kept_so_nothing_is_left_to_do_even_after_reopening() {
    let (data, _assets, mut library) = scanned(PACKAGE);

    let outcomes = run_all(&mut library);

    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    assert!(library.pending_work().is_empty());
    assert!(copy_is_extracted(&library));
    let reopened = Library::open(data.path()).unwrap();
    assert!(reopened.snapshot.catalog.blobs.iter().all(|blob| blob.fingerprint.is_some()));
    assert_eq!(reopened.snapshot.catalog, library.snapshot.catalog);
}

#[test]
fn a_copy_whose_crc_differs_is_no_longer_treated_as_extracted() {
    let (_data, _assets, mut library) = scanned(b"PACKAGE BYTES");
    assert!(copy_is_extracted(&library));

    run_all(&mut library);

    assert!(!copy_is_extracted(&library));
    let member = Task::Package {
        member: Some("Outfit/Outfit.unitypackage".into()),
        bytes: PACKAGE.len() as u64,
    };
    assert_eq!(tasks(&library), [("Outfit.zip".into(), member)], "the zip's own package is read instead");
    run_all(&mut library);
    assert!(library.pending_work().is_empty());
}

#[test]
fn a_file_that_cannot_be_read_fails_alone_and_stays_pending() {
    let (_data, assets, mut library) = scanned(PACKAGE);
    fs::remove_file(assets.path().join("Outfit.zip")).unwrap();

    let outcomes = run_all(&mut library);

    let failed: Vec<_> = outcomes.iter().filter(|outcome| outcome.result.is_err()).collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].work.path.file_name().unwrap(), "Outfit.zip");
    assert_eq!(tasks(&library), [("Outfit.zip".into(), Task::Fingerprint)]);
}

fn unitypackage(paths: &[&str]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (n, path) in paths.iter().enumerate() {
        let guid = format!("{n:032x}");
        for (name, data) in [("pathname", path.as_bytes()), ("asset", b"data".as_slice())] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, format!("{guid}/{name}"), data).unwrap();
        }
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn summaries(library: &Library) -> Vec<(String, usize, Option<String>)> {
    let mut found: Vec<_> = library
        .items()
        .iter()
        .flat_map(|item| library.files(item.id))
        .flat_map(|view| {
            let own = library.package_of(view.file).map(|summary| (view.file.file_name.clone(), summary.clone()));
            let inside = library
                .packages_in(view.blob.id)
                .into_iter()
                .map(|(member, summary)| (member.to_owned(), summary.clone()));
            own.into_iter().chain(inside).collect::<Vec<_>>()
        })
        .map(|(name, summary)| (name, summary.files, summary.root))
        .collect();
    found.sort();
    found
}

#[test]
fn packages_are_read_once_whether_loose_zipped_or_extracted_and_survive_reopening() {
    let data = tempfile::tempdir().unwrap();
    let assets = tempfile::tempdir().unwrap();
    let hair = unitypackage(&["Assets/Shop/Hair/Hair.prefab", "Assets/Shop/Hair/Tex/Hair.png", "Assets/lilToon/Shader/lts.shader"]);
    let outfit = unitypackage(&["Assets/Shop/Outfit/Outfit.prefab"]);
    let ring = unitypackage(&["Assets/Shop/Ring/Ring.prefab"]);
    write(assets.path(), "Hair.unitypackage", &hair);
    write_zip(assets.path(), "Outfit.zip", &[("Outfit/Outfit.unitypackage", &outfit)]);
    write(assets.path(), "Outfit/Outfit/Outfit.unitypackage", &outfit);
    write_zip(assets.path(), "Ring.zip", &[("Ring/Ring.unitypackage", &ring)]);
    let mut library = Library::open(data.path()).unwrap();
    library.scan_root(assets.path(), CodePage::Other).unwrap();

    let reads = run_all(&mut library)
        .iter()
        .filter(|outcome| matches!(outcome.work.task, Task::Package { .. }))
        .count();

    assert_eq!(reads, 3, "the extracted copy stands in for its zip member");
    let expected = vec![
        ("Hair.unitypackage".to_owned(), 3, Some("Assets/Shop/Hair".to_owned())),
        ("Outfit.unitypackage".to_owned(), 1, Some("Assets/Shop/Outfit".to_owned())),
        ("Ring/Ring.unitypackage".to_owned(), 1, Some("Assets/Shop/Ring".to_owned())),
    ];
    assert_eq!(summaries(&library), expected);
    assert!(library.pending_work().is_empty());
    drop(library);
    let mut reopened = Library::open(data.path()).unwrap();
    reopened.scan_root(assets.path(), CodePage::Other).unwrap();
    assert_eq!(summaries(&reopened), expected);
    assert!(reopened.pending_work().is_empty());
}

#[test]
fn a_broken_package_is_remembered_as_failed_and_not_read_again() {
    let (_data, _assets, mut library) = scanned(PACKAGE);

    run_all(&mut library);

    let item = library.items()[0].id;
    let copy = library.files(item).into_iter().find(|view| view.file.file_name == "Outfit.unitypackage").unwrap();
    assert!(library.package_of(copy.file).unwrap().error.is_some());
    assert!(library.pending_work().is_empty());
}
