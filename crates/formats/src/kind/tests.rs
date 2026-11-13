use super::*;

#[test]
fn unitypackage_needs_gzip_header_and_extension() {
    assert_eq!(detect_kind(Some("unitypackage"), &[0x1f, 0x8b, 0x08]), FileKind::UnityPackage);
    assert_eq!(detect_kind(Some("UnityPackage"), &[0x1f, 0x8b, 0x08]), FileKind::UnityPackage);
    assert_eq!(detect_kind(Some("unitypackage"), b"not gzip"), FileKind::Other);
    assert_eq!(detect_kind(Some("gz"), &[0x1f, 0x8b, 0x08]), FileKind::Other);
}

#[test]
fn archives_are_detected_by_header_regardless_of_extension() {
    assert_eq!(detect_kind(Some("zip"), b"PK\x03\x04rest"), FileKind::Archive);
    assert_eq!(detect_kind(None, b"7z\xbc\xaf\x27\x1c"), FileKind::Archive);
    assert_eq!(detect_kind(Some("bin"), b"Rar!\x1a\x07\x01"), FileKind::Archive);
}

#[test]
fn images_include_psd_and_webp() {
    assert_eq!(detect_kind(Some("png"), b"\x89PNG\r\n\x1a\n"), FileKind::Image);
    assert_eq!(detect_kind(Some("jpg"), &[0xff, 0xd8, 0xff, 0xe0]), FileKind::Image);
    assert_eq!(detect_kind(Some("psd"), b"8BPS\x00\x01"), FileKind::Image);
    assert_eq!(detect_kind(Some("webp"), b"RIFF\x00\x00\x00\x00WEBPVP8 "), FileKind::Image);
    assert_eq!(detect_kind(Some("wav"), b"RIFF\x00\x00\x00\x00WAVEfmt "), FileKind::Other);
}

#[test]
fn models_cover_binary_and_ascii_fbx() {
    assert_eq!(detect_kind(Some("fbx"), b"Kaydara FBX Binary  \x00"), FileKind::Model);
    assert_eq!(detect_kind(Some("FBX"), b"; FBX 7.4.0 project file"), FileKind::Model);
    assert_eq!(detect_kind(Some("blend"), b"BLENDER-v300"), FileKind::Model);
    assert_eq!(detect_kind(Some("vrm"), b"glTF\x02\x00\x00\x00"), FileKind::Model);
}

#[test]
fn short_or_empty_headers_fall_back_to_other() {
    assert_eq!(detect_kind(None, &[]), FileKind::Other);
    assert_eq!(detect_kind(Some("txt"), b"R"), FileKind::Other);
}
