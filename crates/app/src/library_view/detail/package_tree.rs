#[derive(Debug, Default, PartialEq, Eq)]
pub struct Entries<'a> {
    pub folders: Vec<String>,
    pub files: Vec<&'a str>,
}

fn children<'a>(paths: &'a [String], folder: &str) -> (Vec<String>, Vec<&'a str>) {
    let mut folders: Vec<String> = Vec::new();
    let mut files = Vec::new();
    for path in paths {
        let Some(rest) = (if folder.is_empty() { Some(path.as_str()) } else { path.strip_prefix(folder).and_then(|rest| rest.strip_prefix('/')) }) else {
            continue;
        };
        match rest.split_once('/') {
            Some((name, _)) => {
                let child = if folder.is_empty() { name.to_owned() } else { format!("{folder}/{name}") };
                if !folders.contains(&child) {
                    folders.push(child);
                }
            }
            None => files.push(path.as_str()),
        }
    }
    folders.sort();
    files.sort();
    (folders, files)
}

fn collapse(paths: &[String], mut folder: String) -> String {
    loop {
        let (inner, files) = children(paths, &folder);
        match inner.as_slice() {
            [only] if files.is_empty() => folder = only.clone(),
            _ => return folder,
        }
    }
}

pub fn top(paths: &[String]) -> String {
    collapse(paths, String::new())
}

pub fn entries<'a>(paths: &'a [String], folder: &str) -> Entries<'a> {
    let (folders, files) = children(paths, folder);
    Entries {
        folders: folders.into_iter().map(|child| collapse(paths, child)).collect(),
        files,
    }
}

pub fn shown<'a>(path: &'a str, parent: &str) -> &'a str {
    if parent.is_empty() {
        path
    } else {
        path.strip_prefix(parent).and_then(|rest| rest.strip_prefix('/')).unwrap_or(path)
    }
}

pub fn holds(folder: &str, path: &str) -> bool {
    path.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests;
