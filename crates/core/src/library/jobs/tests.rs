use std::fs;
use std::io::Write;
use std::path::Path;

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
            ("Outfit.unitypackage".into(), Task::Fingerprint),
            ("Outfit.unitypackage".into(), Task::Crc { expected: crc32fast::hash(PACKAGE) }),
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
