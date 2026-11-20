use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use snowcase_formats::{FileKind, guess_kind, is_junk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFile {
    pub rel_path: String,
    pub size: u64,
    pub modified_ms: i64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkError {
    pub rel_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walk {
    pub files: Vec<FoundFile>,
    pub unity_projects: Vec<String>,
    pub errors: Vec<WalkError>,
}

pub fn walk(root: &Path) -> Walk {
    let mut walk = Walk::default();
    if is_unity_project(root) {
        walk.unity_projects.push(String::new());
        return walk;
    }
    let mut pending = vec![String::new()];
    while let Some(dir) = pending.pop() {
        let entries = match fs::read_dir(root.join(&dir)) {
            Ok(entries) => entries,
            Err(error) => {
                walk.errors.push(WalkError {
                    rel_path: dir,
                    message: error.to_string(),
                });
                continue;
            }
        };
        let mut children = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    walk.errors.push(WalkError {
                        rel_path: dir.clone(),
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(raw) => {
                    walk.errors.push(WalkError {
                        rel_path: join(&dir, &raw.to_string_lossy()),
                        message: "the name is not valid UTF-8".into(),
                    });
                    continue;
                }
            };
            children.push((name, entry));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        let mut subdirs = Vec::new();
        for (name, entry) in children {
            let rel_path = join(&dir, &name);
            if is_junk(&name) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    walk.errors.push(WalkError {
                        rel_path,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            if metadata.is_dir() {
                if is_unity_project(&entry.path()) {
                    walk.unity_projects.push(rel_path);
                } else {
                    subdirs.push(rel_path);
                }
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let extension = Path::new(&name).extension().and_then(|ext| ext.to_str());
            walk.files.push(FoundFile {
                rel_path,
                size: metadata.len(),
                modified_ms: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)),
                kind: guess_kind(extension),
            });
        }
        pending.extend(subdirs.into_iter().rev());
    }
    walk
}

fn is_unity_project(dir: &Path) -> bool {
    dir.join("Assets").is_dir() && dir.join("ProjectSettings").join("ProjectVersion.txt").is_file()
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownFile {
    pub rel_path: String,
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub new: Vec<FoundFile>,
    pub changed: Vec<FoundFile>,
    pub unchanged: usize,
    pub missing: Vec<String>,
}

pub fn diff(known: &[KnownFile], found: &[FoundFile]) -> Changes {
    let mut by_path: HashMap<&str, &KnownFile> =
        known.iter().map(|file| (file.rel_path.as_str(), file)).collect();
    let mut changes = Changes::default();
    for file in found {
        match by_path.remove(file.rel_path.as_str()) {
            None => changes.new.push(file.clone()),
            Some(old) if old.size == file.size && old.modified_ms == file.modified_ms => {
                changes.unchanged += 1;
            }
            Some(_) => changes.changed.push(file.clone()),
        }
    }
    changes.missing = by_path.into_keys().map(str::to_owned).collect();
    changes.missing.sort();
    changes
}

#[cfg(test)]
mod tests;
