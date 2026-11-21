use std::sync::LazyLock;

use serde::Deserialize;

use crate::names::{plain_text, tokens};

#[derive(Deserialize)]
struct Category {
    name: String,
    words: Vec<String>,
}

#[derive(Deserialize)]
struct Vocabulary {
    avatars: Vec<String>,
    category: Vec<Category>,
}

static VOCABULARY: LazyLock<Vocabulary> =
    LazyLock::new(|| toml::from_str(include_str!("vocabulary.toml")).expect("the built-in vocabulary parses"));
static AVATAR_WORDS: LazyLock<Vec<String>> =
    LazyLock::new(|| VOCABULARY.avatars.iter().map(|name| name.to_lowercase()).collect());

pub const AVATAR: &str = "avatar";
const GLUED_WORD: usize = 6;

pub fn is_avatar_word(word: &str) -> bool {
    AVATAR_WORDS.iter().any(|avatar| avatar == word)
}

pub fn categories() -> impl Iterator<Item = &'static str> {
    std::iter::once(AVATAR).chain(VOCABULARY.category.iter().map(|category| category.name.as_str()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recognized {
    pub category: Option<&'static str>,
    pub targets: Vec<&'static str>,
}

struct Name {
    tokens: Vec<String>,
    text: String,
}

impl Name {
    fn new(name: &str) -> Self {
        Self {
            tokens: tokens(name),
            text: plain_text(name),
        }
    }

    fn mentions(&self, word: &str) -> bool {
        if word.is_ascii() {
            self.tokens.iter().any(|token| token == word)
        } else {
            word.chars().count() >= 2 && self.text.contains(word)
        }
    }

    fn avatars(&self) -> impl Iterator<Item = &'static str> + '_ {
        VOCABULARY
            .avatars
            .iter()
            .zip(AVATAR_WORDS.iter())
            .filter(|(_, word)| self.tokens.contains(word))
            .map(|(name, _)| name.as_str())
    }

    fn category(&self) -> Option<&'static str> {
        let glued = |word: &str| word.is_ascii() && word.len() >= GLUED_WORD && self.text.contains(word);
        VOCABULARY
            .category
            .iter()
            .find(|category| category.words.iter().any(|word| self.mentions(word) || glued(word)))
            .map(|category| category.name.as_str())
    }

    fn is_only_an_avatar(&self) -> bool {
        !self.tokens.is_empty()
            && self
                .tokens
                .iter()
                .all(|token| is_avatar_word(token))
    }
}

fn is_picture(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".tga", ".psd"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

pub fn recognize(title: &str, file_names: &[&str]) -> Recognized {
    let title = Name::new(title);
    let files: Vec<Name> = file_names
        .iter()
        .filter(|name| !is_picture(name))
        .map(|name| Name::new(name))
        .collect();
    let has_vrm = file_names.iter().any(|name| name.to_lowercase().ends_with(".vrm"));
    if has_vrm || title.is_only_an_avatar() {
        return Recognized {
            category: Some(AVATAR),
            targets: Vec::new(),
        };
    }
    let mut targets: Vec<&'static str> = Vec::new();
    for name in std::iter::once(&title).chain(&files) {
        for avatar in name.avatars() {
            if !targets.contains(&avatar) {
                targets.push(avatar);
            }
        }
    }
    Recognized {
        category: title.category().or_else(|| files.iter().find_map(Name::category)),
        targets,
    }
}

#[cfg(test)]
mod tests;
