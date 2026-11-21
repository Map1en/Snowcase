use std::collections::{BTreeMap, HashMap};

use snowcase_formats::{ArchiveEntry, FileKind};

use crate::names::{CodePage, name_key, repair_mojibake, title_of, version_of};
use crate::scan::FoundFile;

const SIGNALS: &[&str] = &["unitypackage", "fbx", "blend", "vrm", "psd", "prefab"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub rel_path: String,
    pub display_name: Option<String>,
    pub extracted_from: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub version: Option<String>,
    pub files: Vec<PlannedFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub groups: Vec<Group>,
    pub unsupported: Vec<String>,
}

type Overlap<'a> = HashMap<usize, (u64, Vec<(usize, &'a str)>)>;

struct Unit {
    name: String,
    is_dir: bool,
    files: Vec<usize>,
}

fn extension(path: &str) -> String {
    path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_signal(path: &str) -> bool {
    SIGNALS.contains(&extension(path).as_str())
}

fn units(files: &[FoundFile]) -> Vec<Unit> {
    let mut direct: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut subdirs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (ix, file) in files.iter().enumerate() {
        let path = file.rel_path.as_str();
        let parent = path.rfind('/').map_or("", |cut| &path[..cut]);
        direct.entry(parent).or_default().push(ix);
        let mut dir = parent;
        while !dir.is_empty() {
            let up = dir.rfind('/').map_or("", |cut| &dir[..cut]);
            let children = subdirs.entry(up).or_default();
            if children.contains(&dir) {
                break;
            }
            children.push(dir);
            dir = up;
        }
    }
    let mut units = Vec::new();
    split("", files, &direct, &subdirs, &mut units);
    units
}

fn split(
    dir: &str,
    files: &[FoundFile],
    direct: &BTreeMap<&str, Vec<usize>>,
    subdirs: &BTreeMap<&str, Vec<&str>>,
    units: &mut Vec<Unit>,
) {
    let here = direct.get(dir).map(Vec::as_slice).unwrap_or_default();
    let archives = here.iter().filter(|ix| files[**ix].kind == FileKind::Archive).count();
    if !dir.is_empty() && archives < 2 {
        let prefix = format!("{dir}/");
        units.push(Unit {
            name: file_name(dir).to_owned(),
            is_dir: true,
            files: (0..files.len()).filter(|ix| files[*ix].rel_path.starts_with(&prefix)).collect(),
        });
        return;
    }
    for ix in here {
        units.push(Unit {
            name: file_name(&files[*ix].rel_path).to_owned(),
            is_dir: false,
            files: vec![*ix],
        });
    }
    let mut children = subdirs.get(dir).cloned().unwrap_or_default();
    children.sort();
    for child in children {
        split(child, files, direct, subdirs, units);
    }
}

struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, x: usize) -> usize {
        let parent = self.0[x];
        if parent == x {
            return x;
        }
        let root = self.find(parent);
        self.0[x] = root;
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

pub fn plan(
    files: &[FoundFile],
    listings: &HashMap<String, Vec<ArchiveEntry>>,
    code_page: CodePage,
) -> Plan {
    let display: Vec<Option<String>> = files
        .iter()
        .map(|file| repair_mojibake(file_name(&file.rel_path), code_page))
        .collect();
    let shown = |ix: usize| display[ix].clone().unwrap_or_else(|| file_name(&files[ix].rel_path).to_owned());
    let units = units(files);
    let unit_of: HashMap<usize, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(unit, u)| u.files.iter().map(move |file| (*file, unit)))
        .collect();
    let members = |ix: usize| listings.get(&files[ix].rel_path).map(Vec::as_slice).unwrap_or_default();
    let has_signal = |ix: usize| is_signal(&files[ix].rel_path) || members(ix).iter().any(|m| is_signal(&m.path));

    let mut by_member: HashMap<(String, u64), Vec<(usize, &str)>> = HashMap::new();
    for ix in 0..files.len() {
        if !has_signal(ix) {
            continue;
        }
        for member in members(ix) {
            if member.size > 0 {
                let key = (file_name(&member.path).to_lowercase(), member.size);
                by_member.entry(key).or_default().push((ix, member.path.as_str()));
            }
        }
    }

    let mut sets = Sets((0..units.len()).collect());
    let mut extracted: HashMap<usize, (String, String)> = HashMap::new();
    for (unit, u) in units.iter().enumerate() {
        let total: u64 = u.files.iter().map(|ix| files[*ix].size).sum();
        let mut matched: Overlap = HashMap::new();
        for &ix in &u.files {
            let size = files[ix].size;
            let names = [file_name(&files[ix].rel_path).to_lowercase(), shown(ix).to_lowercase()];
            let hits = names.iter().filter_map(|name| by_member.get(&(name.clone(), size))).next();
            for &(archive, member) in hits.into_iter().flatten() {
                if unit_of[&archive] != unit {
                    let entry = matched.entry(archive).or_default();
                    entry.0 += size;
                    entry.1.push((ix, member));
                }
            }
        }
        let best = matched.into_iter().max_by_key(|(archive, (bytes, _))| (*bytes, std::cmp::Reverse(*archive)));
        if let Some((archive, (bytes, pairs))) = best
            && bytes * 2 >= total
        {
            sets.union(unit, unit_of[&archive]);
            for (ix, member) in pairs {
                extracted.insert(ix, (files[archive].rel_path.clone(), member.to_owned()));
            }
        }
    }

    let names: Vec<String> = units
        .iter()
        .map(|u| match u.is_dir {
            true => repair_mojibake(&u.name, code_page).unwrap_or_else(|| u.name.clone()),
            false => shown(u.files[0]),
        })
        .collect();
    let mut versions: HashMap<usize, String> = HashMap::new();
    for (unit, name) in names.iter().enumerate() {
        if let Some(version) = version_of(name) {
            versions.entry(sets.find(unit)).or_insert(version);
        }
    }
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (unit, name) in names.iter().enumerate() {
        let Some(key) = name_key(name) else {
            continue;
        };
        let root = sets.find(unit);
        let mine = versions.get(&root).cloned();
        let candidates = by_key.entry(key).or_default();
        let other = candidates.iter().map(|c| sets.find(*c)).find(|other| {
            *other == root
                || match (&mine, versions.get(other)) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                }
        });
        match other {
            Some(other) if other == root => {}
            Some(other) => {
                let version = versions.get(&other).cloned().or(mine);
                sets.union(other, root);
                if let Some(version) = version {
                    versions.insert(sets.find(root), version);
                }
            }
            None => candidates.push(root),
        }
    }

    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for unit in 0..units.len() {
        components.entry(sets.find(unit)).or_default().push(unit);
    }
    let mut groups = Vec::new();
    for members_of_group in components.into_values() {
        if !members_of_group.iter().any(|unit| units[*unit].files.iter().any(|ix| has_signal(*ix))) {
            continue;
        }
        let lead = members_of_group
            .iter()
            .copied()
            .max_by_key(|unit| {
                let u = &units[*unit];
                let archive = !u.is_dir && files[u.files[0]].kind == FileKind::Archive && has_signal(u.files[0]);
                (archive, u.is_dir, std::cmp::Reverse(*unit))
            })
            .unwrap();
        let lead_name = &names[lead];
        let version = versions.get(&sets.find(lead)).cloned();
        let mut planned: Vec<PlannedFile> = members_of_group
            .iter()
            .flat_map(|unit| units[*unit].files.iter().copied())
            .map(|ix| PlannedFile {
                rel_path: files[ix].rel_path.clone(),
                display_name: display[ix].clone(),
                extracted_from: extracted.get(&ix).cloned(),
            })
            .collect();
        planned.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        groups.push(Group {
            title: title_of(lead_name),
            version,
            files: planned,
        });
    }

    let mut unsupported: Vec<String> = files
        .iter()
        .filter(|file| extension(&file.rel_path) == "rar")
        .map(|file| file.rel_path.clone())
        .collect();
    unsupported.sort();
    Plan { groups, unsupported }
}

#[cfg(test)]
mod tests;
