#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileKind {
    UnityPackage,
    Archive,
    Image,
    Model,
    Other,
}

pub const HEADER_LEN: usize = 32;

pub fn detect_kind(extension: Option<&str>, header: &[u8]) -> FileKind {
    let extension = extension.map(str::to_ascii_lowercase);
    let extension = extension.as_deref();

    if header.starts_with(&[0x1f, 0x8b]) && extension == Some("unitypackage") {
        return FileKind::UnityPackage;
    }
    if header.starts_with(b"PK\x03\x04")
        || header.starts_with(b"7z\xbc\xaf\x27\x1c")
        || header.starts_with(b"Rar!\x1a\x07")
    {
        return FileKind::Archive;
    }
    if header.starts_with(b"\x89PNG\r\n\x1a\n")
        || header.starts_with(&[0xff, 0xd8, 0xff])
        || header.starts_with(b"GIF8")
        || header.starts_with(b"8BPS")
        || (header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP"))
    {
        return FileKind::Image;
    }
    if header.starts_with(b"Kaydara FBX Binary")
        || header.starts_with(b"BLENDER")
        || header.starts_with(b"glTF")
        || matches!(extension, Some("fbx" | "obj"))
    {
        return FileKind::Model;
    }
    FileKind::Other
}

#[cfg(test)]
mod tests;
