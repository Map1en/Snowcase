use std::io::Cursor;

use encoding_rs::SHIFT_JIS;

use super::*;

const UTF8_FLAG: u16 = 0x0800;

struct Member {
    name: Vec<u8>,
    flags: u16,
    data: Vec<u8>,
    crc32: u32,
}

fn member(name: &str, data: &[u8], crc32: u32) -> Member {
    Member {
        name: name.as_bytes().to_vec(),
        flags: if name.is_ascii() { 0 } else { UTF8_FLAG },
        data: data.to_vec(),
        crc32,
    }
}

fn raw_member(name: &[u8]) -> Member {
    Member {
        name: name.to_vec(),
        flags: 0,
        data: b"data".to_vec(),
        crc32: 7,
    }
}

fn build_zip(members: &[Member]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for m in members {
        let offset = out.len() as u32;
        let size = m.data.len() as u32;
        let name_len = m.name.len() as u16;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for value in [20u16, m.flags, 0, 0, 0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in [m.crc32, size, size] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&m.name);
        out.extend_from_slice(&m.data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for value in [20u16, 20, m.flags, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [m.crc32, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&m.name);
    }
    let central_offset = out.len() as u32;
    let count = members.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for value in [0u16, 0, count, count] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn list(members: &[Member]) -> Vec<ArchiveEntry> {
    list_zip(Cursor::new(build_zip(members))).unwrap()
}

fn sjis(text: &str) -> Vec<u8> {
    SHIFT_JIS.encode(text).0.into_owned()
}

#[test]
fn lists_files_with_size_and_crc_and_skips_directories() {
    let entries = list(&[
        member("Outfit/", b"", 0),
        member("Outfit/Outfit_Manuka.unitypackage", b"package!", 0xdead_beef),
        member("Outfit/Textures/Pink/body.png", b"png", 0x0102_0304),
    ]);

    let summary: Vec<_> = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.size, entry.crc32))
        .collect();
    assert_eq!(
        summary,
        [
            ("Outfit/Outfit_Manuka.unitypackage", 8, 0xdead_beef),
            ("Outfit/Textures/Pink/body.png", 3, 0x0102_0304),
        ]
    );
}

#[test]
fn names_with_the_utf8_flag_are_read_as_utf8() {
    let entries = list(&[member("真冬/マヌカ対応.unitypackage", b"x", 1)]);

    assert_eq!(entries[0].path, "真冬/マヌカ対応.unitypackage");
}

#[test]
fn names_without_the_flag_try_utf8_then_shift_jis() {
    let entries = list(&[
        raw_member("ゆるさいどぽに.unitypackage".as_bytes()),
        raw_member(&sjis("エクステール！.unitypackage")),
        raw_member(&sjis("読んでくれたら助かるﾖ.txt")),
    ]);

    let names: Vec<_> = entries.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(
        names,
        [
            "ゆるさいどぽに.unitypackage",
            "エクステール！.unitypackage",
            "読んでくれたら助かるﾖ.txt",
        ]
    );
}

#[test]
fn undecodable_names_still_list_instead_of_failing() {
    let entries = list(&[raw_member(&[b'a', 0x82, 0xff, b'.', b'p', b'n', b'g'])]);

    assert!(entries[0].path.ends_with(".png"));
}

#[test]
fn backslashes_from_windows_tools_become_slashes() {
    let entries = list(&[member("FBX\\ribbon.fbx", b"fbx", 1)]);

    assert_eq!(entries[0].path, "FBX/ribbon.fbx");
}

#[test]
fn a_file_that_is_not_a_zip_fails_with_an_error() {
    assert!(list_zip(Cursor::new(b"not a zip at all".to_vec())).is_err());
    assert!(list_zip(Cursor::new(Vec::new())).is_err());
}

#[test]
fn macos_metadata_is_junk_but_ordinary_names_are_not() {
    assert!(is_junk("__MACOSX/C_Halo/._license.pdf"));
    assert!(is_junk("bh01boost.unitypackage/__MACOSX/x"));
    assert!(is_junk("._bh01boost.unitypackage"));
    assert!(is_junk("Outfit\\._Outfit.unitypackage"));
    assert!(!is_junk("Outfit/.gitkeep"));
    assert!(!is_junk("MACOSX_theme/readme.txt"));
    assert!(!is_junk("Outfit/Outfit.unitypackage"));
}

fn build_7z(files: &[(&str, &[u8])], directories: &[&str]) -> Vec<u8> {
    use sevenz_rust2::{ArchiveEntry as SevenEntry, ArchiveWriter};
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for name in directories {
        writer
            .push_archive_entry::<&[u8]>(SevenEntry::new_directory(name), None)
            .unwrap();
    }
    for (name, data) in files {
        writer
            .push_archive_entry(SevenEntry::new_file(name), Some(*data))
            .unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn lists_7z_files_with_size_and_crc_and_skips_directories() {
    let bytes = build_7z(
        &[
            ("mafuyu-meeting/MafuyuMini_ver1.012.unitypackage", b"package!"),
            ("mafuyu-meeting\\読んでね.txt", b"readme"),
        ],
        &["mafuyu-meeting"],
    );

    let entries = list_7z(Cursor::new(bytes)).unwrap();

    let summary: Vec<_> = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.size, entry.crc32))
        .collect();
    assert_eq!(
        summary,
        [
            ("mafuyu-meeting/MafuyuMini_ver1.012.unitypackage", 8, crc32(b"package!")),
            ("mafuyu-meeting/読んでね.txt", 6, crc32(b"readme")),
        ]
    );
}

#[test]
fn a_file_that_is_not_a_7z_fails_with_an_error() {
    assert!(list_7z(Cursor::new(build_zip(&[member("a.txt", b"a", 1)]))).is_err());
    assert!(list_7z(Cursor::new(Vec::new())).is_err());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
