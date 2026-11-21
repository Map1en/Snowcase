use std::sync::LazyLock;

use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS};
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::recognize::avatar_aliases;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodePage {
    Japanese,
    SimplifiedChinese,
    TraditionalChinese,
    Korean,
    Other,
}

impl CodePage {
    pub fn from_ansi(code: u32) -> Self {
        match code {
            932 => Self::Japanese,
            936 => Self::SimplifiedChinese,
            950 => Self::TraditionalChinese,
            949 => Self::Korean,
            _ => Self::Other,
        }
    }

    fn misread_as(self) -> Option<&'static Encoding> {
        match self {
            Self::SimplifiedChinese => Some(GBK),
            Self::TraditionalChinese => Some(BIG5),
            Self::Korean => Some(EUC_KR),
            Self::Japanese | Self::Other => None,
        }
    }
}

fn is_kana(c: char) -> bool {
    ('\u{3040}'..='\u{30ff}').contains(&c)
}

pub fn repair_mojibake(name: &str, code_page: CodePage) -> Option<String> {
    let misread = code_page.misread_as()?;
    if name.is_ascii() || name.chars().any(is_kana) {
        return None;
    }
    let (bytes, _, unmappable) = misread.encode(name);
    if unmappable {
        return None;
    }
    let repaired = SHIFT_JIS.decode_without_bom_handling_and_without_replacement(&bytes)?;
    repaired.chars().any(is_kana).then(|| repaired.into_owned())
}

static EXTENSIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\.(unitypackage|zip|7z|rar|fbx|psd|blend|vrm|png|jpe?g|txt|pdf))+$").unwrap()
});
static DUPLICATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\(\d+\)$").unwrap());
static PREFIXED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|[^a-z0-9])(?:version|ver|v)[.\s_-]*(\d+(?:[._]\d+)*(?:[-.]?(?:rc|beta|alpha)\.?\d*)?)").unwrap()
});
static GLUED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"()(?:(?i:ver)|V)(\d+(?:[._]\d+)+)").unwrap());
static TRAILING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^0-9_])_(\d+(?:_\d+)+)$").unwrap());
static DOTTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^0-9.])(\d+(?:\.\d+)+)").unwrap());
static SEPARATOR_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ _-]{2,}").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^0-9])(20\d{6}(?:\d{6})?)([^0-9]|$)").unwrap());

const ROLE_WORDS: &[&str] = &[
    "material", "materials", "prefab", "prefabs", "texture", "textures", "tex", "fullset", "full", "psd",
    "all", "for",
];
fn base_name(name: &str) -> String {
    let name: String = name.nfkc().collect();
    let name = EXTENSIONS.replace(&name, "");
    DUPLICATE.replace(&name, "").into_owned()
}

fn find_version(name: &str) -> Option<(usize, usize, String)> {
    for pattern in [&*PREFIXED, &*GLUED, &*TRAILING, &*DOTTED] {
        if let Some(captures) = pattern.captures(name) {
            let start = captures.get(1).map_or(0, |prefix| prefix.end());
            let value = captures.get(2).unwrap();
            let end = value.end();
            let text = value.as_str().replace('_', ".").to_lowercase();
            return Some((start, end, text));
        }
    }
    None
}

pub fn version_of(name: &str) -> Option<String> {
    let name = base_name(name);
    let name = DATE.replace_all(&name, "${1}${3}");
    find_version(&name).map(|(_, _, text)| text)
}

fn without_versions(name: &str) -> String {
    let mut name = DATE.replace_all(name, "${1}${3}").into_owned();
    while let Some((start, end, _)) = find_version(&name) {
        name.replace_range(start..end, " ");
    }
    name
}

pub fn tag_key(name: &str) -> String {
    name.trim().nfkc().collect::<String>().to_lowercase()
}

pub fn title_of(name: &str) -> String {
    let stem = DUPLICATE.replace(&EXTENSIONS.replace(name, ""), "").into_owned();
    let title = without_versions(&stem);
    let title = SEPARATOR_RUN.replace_all(&title, |run: &regex::Captures| {
        let run = &run[0];
        if run.contains('_') { "_" } else if run.contains('-') { "-" } else { " " }
    });
    let title = title.trim_matches(|c: char| c.is_whitespace() || "_-.".contains(c));
    if title.is_empty() { stem } else { title.to_owned() }
}

fn words(piece: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Class {
        Upper,
        Lower,
        Digit,
        Wide,
    }
    let class = |c: char| match c {
        'A'..='Z' => Class::Upper,
        'a'..='z' => Class::Lower,
        '0'..='9' => Class::Digit,
        _ => Class::Wide,
    };
    let chars: Vec<char> = piece.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (ix, &c) in chars.iter().enumerate() {
        if let Some(&prev) = ix.checked_sub(1).and_then(|p| chars.get(p)) {
            let (a, b) = (class(prev), class(c));
            let next_lower = chars.get(ix + 1).is_some_and(|n| class(*n) == Class::Lower);
            let split = match (a, b) {
                (Class::Upper, Class::Lower) => false,
                (Class::Upper, Class::Upper) => next_lower,
                (a, b) => a != b,
            };
            if split && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || c.is_ascii_punctuation() || "【】「」『』・".contains(c)
}

pub(crate) fn plain_text(name: &str) -> String {
    without_versions(&base_name(name)).to_lowercase()
}

pub(crate) fn tokens(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for piece in without_versions(&base_name(name)).split(is_separator).filter(|piece| !piece.is_empty()) {
        let lower = piece.to_lowercase();
        let words = words(piece);
        if words.len() > 1 {
            tokens.push(lower);
        }
        tokens.extend(words.into_iter().map(|word| word.to_lowercase()));
    }
    tokens
}

pub fn name_key(name: &str) -> Option<String> {
    let name = without_versions(&base_name(name));
    let mut stripped = false;
    let mut key = String::new();
    for piece in name.split(is_separator).filter(|piece| !piece.is_empty()) {
        if ROLE_WORDS.contains(&piece.to_lowercase().as_str()) {
            stripped = true;
            continue;
        }
        for word in words(piece) {
            let lower = word.to_lowercase();
            if ROLE_WORDS.contains(&lower.as_str()) || avatar_aliases().any(|alias| alias == lower) {
                stripped = true;
                continue;
            }
            let mut rest = lower;
            for avatar in avatar_aliases().filter(|avatar| !avatar.is_ascii()) {
                if rest.contains(avatar) {
                    rest = rest.replace(avatar, "");
                    stripped = true;
                }
            }
            key.push_str(&rest);
        }
    }
    let weight: usize = key.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
    let needed = if stripped { 8 } else { 5 };
    (weight >= needed).then_some(key)
}

#[cfg(test)]
mod tests;
