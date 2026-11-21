use std::collections::BTreeMap;
use std::io::Read;

use flate2::read::GzDecoder;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageAsset {
    pub guid: String,
    pub path: String,
    pub is_file: bool,
    pub has_preview: bool,
}

pub fn list_unitypackage(reader: impl Read) -> std::io::Result<Vec<PackageAsset>> {
    let mut archive = tar::Archive::new(GzDecoder::new(reader));
    let mut assets: BTreeMap<String, PackageAsset> = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let name = name.strip_prefix("./").unwrap_or(&name);
        let Some((guid, part)) = name.split_once('/') else {
            continue;
        };
        if !is_guid(guid) {
            continue;
        }
        let asset = assets.entry(guid.to_owned()).or_insert_with(|| PackageAsset {
            guid: guid.to_owned(),
            ..PackageAsset::default()
        });
        match part {
            "pathname" => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                let text = String::from_utf8_lossy(&bytes);
                asset.path = text.lines().next().unwrap_or_default().trim_end_matches('\r').to_owned();
            }
            "asset" => asset.is_file = true,
            "preview.png" => asset.has_preview = true,
            _ => {}
        }
    }
    Ok(assets.into_values().filter(|asset| !asset.path.is_empty()).collect())
}

fn is_guid(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
