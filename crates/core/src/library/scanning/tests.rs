use std::fs;
use std::io::Write;
use std::path::Path;

use snowcase_store::LocationState;
use zip::write::SimpleFileOptions;

use super::*;
use crate::library::Snapshot;

const PACKAGE: &[u8] = b"package bytes";
const TEXTURE: &[u8] = b"png bytes!";

fn write(root: &Path, rel_path: &str, bytes: &[u8]) {
    let path = root.join(rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn write_zip(root: &Path, rel_path: &str, members: &[(&str, &[u8])]) {
    let path = root.join(rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in members {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn outfit(root: &Path) {
    write_zip(root, "Outfit_v1.0.zip", &[("Outfit/Outfit.unitypackage", PACKAGE), ("Outfit/body.png", TEXTURE)]);
    write(root, "Outfit_v1.0/Outfit/Outfit.unitypackage", PACKAGE);
    write(root, "Outfit_v1.0/Outfit/body.png", TEXTURE);
}

struct Fixture {
    _data: tempfile::TempDir,
    assets: tempfile::TempDir,
    library: Library,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let library = Library::open(data.path()).unwrap();
    Fixture {
        _data: data,
        assets: tempfile::tempdir().unwrap(),
        library,
    }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.assets.path()
    }

    fn scan(&mut self) -> ScanReport {
        let root = self.assets.path().to_path_buf();
        self.library.scan_root(&root, CodePage::SimplifiedChinese).unwrap()
    }

    fn only_item(&self) -> ItemId {
        assert_eq!(self.library.items().len(), 1);
        self.library.items()[0].id
    }

    fn files(&self, item: ItemId) -> Vec<(String, bool, LocationState)> {
        let mut files: Vec<_> = self
            .library
            .files(item)
            .iter()
            .map(|view| {
                let location = view.locations.first().unwrap();
                (location.rel_path.clone(), view.file.extracted_from_blob_id.is_some(), location.state)
            })
            .collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }
}

fn assert_matches_a_full_reload(library: &Library, dir: &Path) {
    let incremental: Arc<Snapshot> = library.snapshot();
    let reloaded = Library::open(dir).unwrap().snapshot();
    assert_eq!(incremental.catalog, reloaded.catalog);
    assert_eq!(incremental.facts, reloaded.facts);
}

#[test]
fn the_first_scan_groups_files_into_items_and_reports_what_it_skipped() {
    let mut fx = fixture();
    outfit(fx.root());
    write(fx.root(), "setup.exe", b"exe");
    write(fx.root(), "TENSHI_CHOKER.rar", b"rar");
    write(fx.root(), "Proj/Assets/Shop/Outfit.prefab", b"prefab");
    write(fx.root(), "Proj/ProjectSettings/ProjectVersion.txt", b"2022.3");

    let report = fx.scan();

    assert_eq!((report.new_items, report.new_files), (1, 3));
    assert_eq!(report.unity_projects, ["Proj"]);
    assert_eq!(report.unsupported, ["TENSHI_CHOKER.rar"]);
    let item = fx.only_item();
    assert_eq!(fx.library.item(item).unwrap().title, "Outfit");
    assert_eq!(
        fx.files(item),
        [
            ("Outfit_v1.0.zip".into(), false, LocationState::Present),
            ("Outfit_v1.0/Outfit/Outfit.unitypackage".into(), true, LocationState::Present),
            ("Outfit_v1.0/Outfit/body.png".into(), true, LocationState::Present),
        ]
    );
    assert_eq!(
        fx.library.main_file(item).map(|file| file.file_name.as_str()),
        Some("Outfit_v1.0.zip"),
        "the original wins over its extracted copies"
    );
    let zip = fx.library.files(item).into_iter().find(|view| view.file.file_name == "Outfit_v1.0.zip").unwrap();
    let node = FacetNode {
        item_id: item,
        blob_id: Some(zip.blob.id),
        inner_path: None,
    };
    assert_eq!(fx.library.facets(&node, FacetKey::Version)[0].value, "1.0");
    assert_eq!(
        fx.library.location_path(zip.locations[0]),
        fx.root().join("Outfit_v1.0.zip").to_string_lossy()
    );
}

#[test]
fn scanning_again_without_changes_changes_nothing() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let before = fx.library.snapshot();

    let report = fx.scan();

    assert_eq!(report, ScanReport::default());
    assert_eq!(before.catalog, fx.library.snapshot().catalog);
}

#[test]
fn a_new_version_becomes_its_own_item_with_its_version() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let old = fx.only_item();

    write_zip(fx.root(), "Outfit_v1.1.zip", &[("Outfit/Outfit.unitypackage", b"newer package")]);
    let report = fx.scan();

    assert_eq!((report.new_items, report.new_files), (1, 1));
    assert_eq!(fx.library.files(old).len(), 3);
    let mut versions: Vec<String> = fx
        .library
        .items()
        .iter()
        .map(|item| fx.library.facets(&crate::library::item_node(item.id), FacetKey::Version)[0].value.clone())
        .collect();
    versions.sort();
    assert_eq!(versions, ["1.0", "1.1"]);
}

#[test]
fn a_moved_file_keeps_its_blob_and_only_changes_location() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let item = fx.only_item();
    let blobs: Vec<_> = fx.library.files(item).iter().map(|view| view.blob.id).collect();

    fs::create_dir(fx.root().join("Outfits")).unwrap();
    fs::rename(fx.root().join("Outfit_v1.0.zip"), fx.root().join("Outfits/Outfit_v1.0.zip")).unwrap();
    let report = fx.scan();

    assert_eq!((report.moved, report.new_files, report.missing), (1, 0, 0));
    let mut after: Vec<_> = fx.library.files(item).iter().map(|view| view.blob.id).collect();
    after.sort();
    let mut before = blobs;
    before.sort();
    assert_eq!(after, before);
    assert!(fx.files(item).iter().any(|(path, _, _)| path == "Outfits/Outfit_v1.0.zip"));
}

#[test]
fn deleting_an_extracted_copy_silently_drops_it() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let item = fx.only_item();

    fs::remove_dir_all(fx.root().join("Outfit_v1.0")).unwrap();
    let report = fx.scan();

    assert_eq!((report.removed_copies, report.missing), (2, 0));
    assert_eq!(fx.files(item), [("Outfit_v1.0.zip".into(), false, LocationState::Present)]);
}

#[test]
fn deleting_the_zip_promotes_its_extracted_copies() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let item = fx.only_item();

    fs::remove_file(fx.root().join("Outfit_v1.0.zip")).unwrap();
    let report = fx.scan();

    assert_eq!((report.missing, report.removed_copies), (1, 0));
    assert_eq!(
        fx.files(item),
        [
            ("Outfit_v1.0.zip".into(), false, LocationState::Missing),
            ("Outfit_v1.0/Outfit/Outfit.unitypackage".into(), false, LocationState::Present),
            ("Outfit_v1.0/Outfit/body.png".into(), false, LocationState::Present),
        ]
    );
}

#[test]
fn a_missing_file_that_comes_back_is_present_again() {
    let mut fx = fixture();
    write(fx.root(), "Hair.unitypackage", PACKAGE);
    fx.scan();
    let item = fx.only_item();
    let outside = tempfile::tempdir().unwrap();
    let saved = outside.path().join("Hair.unitypackage");
    fs::rename(fx.root().join("Hair.unitypackage"), &saved).unwrap();
    assert_eq!(fx.scan().missing, 1);

    fs::rename(&saved, fx.root().join("Hair.unitypackage")).unwrap();
    let report = fx.scan();

    assert_eq!(report.new_files, 0);
    assert_eq!(fx.files(item), [("Hair.unitypackage".into(), false, LocationState::Present)]);
}

#[test]
fn every_rescan_keeps_the_snapshot_equal_to_a_full_reload() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    write_zip(fx.root(), "Outfit_v1.1.zip", &[("Outfit/Outfit.unitypackage", b"newer package")]);
    write(fx.root(), "Gloves.unitypackage", PACKAGE);
    fx.scan();
    fs::remove_dir_all(fx.root().join("Outfit_v1.0")).unwrap();
    fs::remove_file(fx.root().join("Gloves.unitypackage")).unwrap();
    fx.scan();

    assert_matches_a_full_reload(&fx.library, fx._data.path());
}

#[test]
fn scanning_every_root_picks_up_new_files_and_skips_a_root_that_is_not_there() {
    let mut fx = fixture();
    outfit(fx.root());
    fx.scan();
    let unplugged = tempfile::tempdir().unwrap();
    write(unplugged.path(), "Gloves.unitypackage", PACKAGE);
    fx.library.scan_root(unplugged.path(), CodePage::SimplifiedChinese).unwrap();
    let unplugged_path = unplugged.path().to_path_buf();
    drop(unplugged);
    write(fx.root(), "TENSHI_CHOKER.rar", b"rar");
    write(fx.root(), "Hair.unitypackage", PACKAGE);

    let report = fx.library.scan_roots(CodePage::SimplifiedChinese).unwrap();

    assert_eq!((report.new_items, report.missing), (1, 0), "the missing root's files are not marked missing");
    assert_eq!(report.unsupported, [fx.root().join("TENSHI_CHOKER.rar").to_string_lossy()]);
    assert!(!unplugged_path.exists());
    let states: Vec<LocationState> = fx
        .library
        .items()
        .iter()
        .flat_map(|item| fx.library.files(item.id))
        .map(|view| view.locations[0].state)
        .collect();
    assert!(states.iter().all(|state| *state == LocationState::Present));
}

#[test]
fn removing_a_root_drops_items_that_live_only_there_and_keeps_the_rest() {
    let mut fx = fixture();
    write(fx.root(), "Hair.unitypackage", PACKAGE);
    write(fx.root(), "Gloves.unitypackage", b"gloves bytes");
    fx.scan();
    let other = tempfile::tempdir().unwrap();
    write(other.path(), "Ribbon.unitypackage", TEXTURE);
    fx.library.scan_root(other.path(), CodePage::SimplifiedChinese).unwrap();
    let id = |library: &Library, title: &str| library.items().iter().find(|item| item.title == title).unwrap().id;
    let (hair, ribbon) = (id(&fx.library, "Hair"), id(&fx.library, "Ribbon"));
    fx.library.merge_items(ribbon, hair).unwrap();
    let root = fx.library.roots()[0].0.id;
    assert_eq!(fx.library.only_in_root(root), 1, "only Gloves lives entirely in the first root");

    fx.library.remove_root(root).unwrap();

    let titles: Vec<&str> = fx.library.items().iter().map(|item| item.title.as_str()).collect();
    assert_eq!(titles, ["Ribbon"]);
    assert_eq!(fx.library.files(ribbon).len(), 1, "the merged file from the removed root is gone");
    assert_eq!(fx.library.roots().len(), 1);
    assert!(fx.root().join("Hair.unitypackage").exists(), "files on disk are left alone");
    assert_matches_a_full_reload(&fx.library, fx._data.path());
}
