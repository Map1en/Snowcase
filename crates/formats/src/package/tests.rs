use std::io::Cursor;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::*;

const HAIR: &str = "0123456789abcdef0123456789abcdef";
const FOLDER: &str = "fedcba9876543210fedcba9876543210";

fn package(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (name, data) in members {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

#[test]
fn lists_each_guid_with_its_path_and_whether_it_is_a_file_with_a_preview() {
    let bytes = package(&[
        (&format!("{HAIR}/asset"), b"png"),
        (&format!("{HAIR}/asset.meta"), b"guid"),
        (&format!("{HAIR}/pathname"), b"Assets/PLUSONE/Mafuyu/Hair.png\n00"),
        (&format!("{HAIR}/preview.png"), b"png"),
        (&format!("{FOLDER}/pathname"), "Assets/PLUSONE/Mafuyu　Variant".as_bytes()),
    ]);

    let assets = list_unitypackage(Cursor::new(bytes)).unwrap();

    assert_eq!(
        assets,
        [
            PackageAsset {
                guid: HAIR.into(),
                path: "Assets/PLUSONE/Mafuyu/Hair.png".into(),
                is_file: true,
                has_preview: true,
            },
            PackageAsset {
                guid: FOLDER.into(),
                path: "Assets/PLUSONE/Mafuyu　Variant".into(),
                is_file: false,
                has_preview: false,
            },
        ]
    );
}

#[test]
fn dot_slash_prefixes_apple_double_members_and_stray_files_are_handled() {
    let bytes = package(&[
        (&format!("./{HAIR}/pathname"), b"Assets/C_Halo/Halo.prefab"),
        (&format!("./{HAIR}/asset"), b"prefab"),
        (&format!("./{HAIR}/._asset"), b"apple"),
        ("._icon.png", b"apple"),
        (".icon.png", b"icon"),
    ]);

    let assets = list_unitypackage(Cursor::new(bytes)).unwrap();

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].path, "Assets/C_Halo/Halo.prefab");
    assert!(assets[0].is_file);
}

#[test]
fn a_file_that_is_not_gzip_is_an_error() {
    assert!(list_unitypackage(Cursor::new(b"Mac OS X AppleDouble".to_vec())).is_err());
}
