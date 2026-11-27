
use snowcase_store::{FacetKey, FolderId, Item, ItemFile, ItemId, RootId, Tag, TagId};

use super::{Snapshot, shown_name};
use super::facts::tally;
use crate::ingest::names::tag_key;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Unlinked,
    Folder(FolderId),
    Root(RootId),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub scope: Scope,
    pub search: String,
    pub category: Option<String>,
    pub target: Option<String>,
    pub shop: Option<String>,
    pub tag: Option<TagId>,
}

impl Filter {
    pub fn search(search: &str) -> Self {
        Self {
            search: search.to_owned(),
            ..Self::default()
        }
    }

    pub fn scope(scope: Scope) -> Self {
        Self {
            scope,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    File(String),
    Package { file: String, package: String, path: String },
    Tag { name: String, file: Option<String> },
    Attribute(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Added,
    Name,
    Size,
}

impl Snapshot {
    pub fn query(&self, filter: &Filter, sort: SortKey) -> Vec<&Item> {
        let needle = filter.search.trim().to_lowercase();
        let mut found: Vec<&Item> = self
            .catalog
            .items
            .iter()
            .filter(|item| self.matches(item, filter, &needle))
            .collect();
        match sort {
            SortKey::Added => found.sort_by_key(|item| std::cmp::Reverse((item.created_at, item.id))),
            SortKey::Name => found.sort_by_key(|item| (item.title.to_lowercase(), item.id)),
            SortKey::Size => found.sort_by_key(|item| {
                std::cmp::Reverse((self.facts.get(&item.id).map_or(0, |facts| facts.size), item.id))
            }),
        }
        found
    }

    pub fn count(&self, scope: &Scope) -> usize {
        self.catalog
            .items
            .iter()
            .filter(|item| self.in_scope(item.id, scope))
            .count()
    }

    fn in_scope(&self, id: ItemId, scope: &Scope) -> bool {
        let Some(facts) = self.facts.get(&id) else {
            return false;
        };
        match scope {
            Scope::All => true,
            Scope::Unlinked => self.booth_page(id).is_none(),
            Scope::Folder(folder) => facts.folders.contains(folder),
            Scope::Root(root) => facts.roots.contains(root),
        }
    }

    fn matches(&self, item: &Item, filter: &Filter, needle: &str) -> bool {
        let Some(facts) = self.facts.get(&item.id) else {
            return false;
        };
        let wanted = |key, value: &Option<String>| value.as_ref().is_none_or(|value| facts.has(key, value));
        let fits = self.in_scope(item.id, &filter.scope)
            && wanted(FacetKey::Category, &filter.category)
            && wanted(FacetKey::Target, &filter.target)
            && filter.shop.as_ref().is_none_or(|shop| facts.shop.as_ref() == Some(shop))
            && filter.tag.is_none_or(|tag| facts.all_tags.contains(&tag));
        fits && (needle.is_empty()
            || item.title.to_lowercase().contains(needle)
            || facts.shop.as_ref().is_some_and(|shop| shop.to_lowercase().contains(needle))
            || facts.store_titles.iter().any(|title| title.to_lowercase().contains(needle))
            || self.hit(item.id, needle).is_some())
    }

    pub fn hit(&self, id: ItemId, search: &str) -> Option<Hit> {
        let needle = search.trim().to_lowercase();
        let facts = self.facts.get(&id)?;
        if needle.is_empty() {
            return None;
        }
        let found = |text: &str| text.to_lowercase().contains(&needle);
        let any_tag = facts.all_tags.iter().any(|tag| self.tag(*tag).is_some_and(|tag| found(&tag.name)));
        let tagged = self
            .node_tags_of(id)
            .filter(|_| any_tag)
            .find_map(|link| {
                let tag = self.tag(link.tag_id).filter(|tag| found(&tag.name))?;
                let file = link.blob_id.and_then(|blob| {
                    let file = self.item_file(id, blob)?;
                    Some(link.inner_path.clone().unwrap_or_else(|| shown_name(file).to_owned()))
                });
                Some(Hit::Tag {
                    name: tag.name.clone(),
                    file,
                })
            });
        let packaged = || {
            self.item_files(id).iter().find_map(|file| {
                let own = self.package_of(file).map(|summary| (shown_name(file), summary));
                let inside = self
                    .packages_in(file.blob_id)
                    .into_iter()
                    .map(|(member, summary)| (member.rsplit('/').next().unwrap_or(member), summary));
                own.into_iter().chain(inside).find_map(|(package, summary)| {
                    let mut matching = summary.paths.iter().filter(|path| found(path));
                    let first = matching.next()?;
                    let path = std::iter::once(first)
                        .chain(matching)
                        .find(|path| path.to_lowercase().ends_with(".prefab"))
                        .unwrap_or(first);
                    Some(Hit::Package {
                        file: shown_name(file).to_owned(),
                        package: package.to_owned(),
                        path: path.clone(),
                    })
                })
            })
        };
        tagged
            .or_else(|| facts.file_names.iter().find(|name| found(name)).cloned().map(Hit::File))
            .or_else(packaged)
            .or_else(|| {
                facts
                    .attributes
                    .iter()
                    .find(|(_, value)| found(value))
                    .map(|(_, value)| Hit::Attribute(value.clone()))
            })
    }

    pub fn main_file(&self, id: ItemId) -> Option<&ItemFile> {
        let size = |file: &ItemFile| self.blob(file.blob_id).map_or(0, |blob| blob.size);
        self.item_files(id)
            .iter()
            .max_by_key(|file| (file.extracted_from_blob_id.is_none(), size(file), std::cmp::Reverse(file.blob_id)))
    }

    pub fn suggest_tags(&self, draft: &str) -> Vec<(&Tag, usize)> {
        let key = tag_key(draft);
        let mut tags: Vec<(&Tag, usize)> = self
            .tag_counts()
            .into_iter()
            .filter(|(tag, _)| {
                key.is_empty()
                    || tag.key != key
                        && (tag.key.contains(&key) && key.chars().count() >= 2 || key.contains(&tag.key))
            })
            .collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        tags
    }

    pub fn values(&self, key: FacetKey) -> Vec<(String, usize)> {
        tally(self.facts.values().flat_map(|facts| {
            facts
                .attributes
                .iter()
                .filter(move |(candidate, _)| *candidate == key)
                .map(|(_, value)| value.clone())
        }))
    }

    pub fn shops(&self) -> Vec<(String, usize)> {
        tally(self.facts.values().filter_map(|facts| facts.shop.clone()))
    }
}
