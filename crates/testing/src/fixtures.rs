use fake::Fake;
use fake::faker::lorem::en::Word;
use fake::faker::name::ja_jp::FirstName;
use rand::SeedableRng;
use rand::rngs::StdRng;
use snowcase_core::FileKind;
use snowcase_core::library::{FileInfo, Library};
use snowcase_core::{FolderId, ItemChanges, ItemStatus};

const NOUNS: [&str; 8] = [
    "Avatar", "Outfit", "Earrings", "Hair", "Shader", "Tool", "Textures", "Set",
];
const SUFFIXES: [&str; 6] = ["", "_v1.2", "_Kikyo", "_Manuka", "_PSD", "_Quest"];
const FOLDERS: [&str; 3] = ["Avatars", "Outfits", "Shaders"];
const TAGS: [&str; 4] = ["favorite", "kikyo", "manuka", "quest"];

pub struct Fixture {
    pub library: Library,
    pub dir: tempfile::TempDir,
}

pub fn empty() -> Fixture {
    let dir = tempfile::tempdir().expect("create fixture directory");
    let library = Library::open(dir.path()).expect("open fixture library");
    Fixture { library, dir }
}

pub const ASSET_FILES: [&str; 2] = ["Outfit.unitypackage", "cover.png"];

pub fn asset_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create asset directory");
    std::fs::write(dir.path().join(ASSET_FILES[0]), [0x1f, 0x8b, 0x08]).expect("write package");
    std::fs::write(dir.path().join(ASSET_FILES[1]), [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).expect("write image");
    dir
}

pub fn library(seed: u64, count: usize) -> Fixture {
    let dir = tempfile::tempdir().expect("create fixture directory");
    let mut library = Library::open(dir.path()).expect("open fixture library");
    let mut rng = StdRng::seed_from_u64(seed);
    library
        .import(files(&mut rng, count))
        .expect("import fixture files");

    let roots: Vec<FolderId> = FOLDERS
        .iter()
        .filter_map(|name| library.create_folder(None, name).expect("create folder"))
        .collect();
    library
        .create_folder(roots.first().copied(), "Kikyo")
        .expect("create nested folder");
    let folders: Vec<FolderId> = library.folder_tree().iter().map(|row| row.folder.id).collect();

    let ids: Vec<_> = library.items().iter().map(|item| item.id).collect();
    for id in ids {
        let status = if pick(&mut rng, 10) < 3 {
            ItemStatus::Inbox
        } else {
            ItemStatus::Confirmed
        };
        library
            .update_item(
                id,
                &ItemChanges {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .expect("update fixture item");
        if pick(&mut rng, 2) == 0 {
            let folder = folders[pick(&mut rng, folders.len())];
            library.put_in_folder(id, folder).expect("file fixture item");
        }
        for _ in 0..pick(&mut rng, 3) {
            library
                .tag_item(id, TAGS[pick(&mut rng, TAGS.len())])
                .expect("tag fixture item");
        }
    }
    Fixture { library, dir }
}

fn files(rng: &mut StdRng, count: usize) -> Vec<FileInfo> {
    (0..count)
        .map(|n| {
            let noun = NOUNS[pick(rng, NOUNS.len())];
            let title = match pick(rng, 20) {
                0 => FirstName().fake_with_rng::<String, _>(rng),
                1 => format!("{} {noun} Deluxe Edition With Extra Long Name", word(rng)),
                _ => format!("{} {noun}", word(rng)),
            };
            let (kind, extension) = match pick(rng, 10) {
                0..=5 => (FileKind::UnityPackage, "unitypackage"),
                6 | 7 => (FileKind::Archive, "zip"),
                8 => (FileKind::Image, "png"),
                _ => (FileKind::Model, "fbx"),
            };
            let suffix = SUFFIXES[pick(rng, SUFFIXES.len())];
            let name = format!("{}{suffix}.{extension}", title.replace(' ', ""));
            FileInfo {
                path: format!("D:/Fixtures/{n:05}/{name}"),
                name,
                size: (1..400u64).fake_with_rng::<u64, _>(rng) * 1024 * 1024,
                modified_ms: (1_600_000_000_000..1_800_000_000_000i64).fake_with_rng(rng),
                kind,
            }
        })
        .collect()
}

fn word(rng: &mut StdRng) -> String {
    let word: String = Word().fake_with_rng(rng);
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or(word)
}

fn pick(rng: &mut StdRng, len: usize) -> usize {
    (0..len).fake_with_rng(rng)
}

#[cfg(test)]
mod tests;
