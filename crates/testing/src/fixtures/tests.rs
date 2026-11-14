use super::*;
use snowcase_core::library::{Scope, SortKey};

fn snapshot(fixture: &Fixture) -> Vec<(String, ItemStatus, usize)> {
    let library = &fixture.library;
    library
        .query(&Scope::All, "", SortKey::Name)
        .into_iter()
        .map(|item| {
            let facts = library.facts(item.id).unwrap();
            (item.title.clone(), item.status, facts.tags.len())
        })
        .collect()
}

#[test]
fn the_same_seed_builds_the_same_library() {
    assert_eq!(snapshot(&library(7, 30)), snapshot(&library(7, 30)));
    assert_ne!(snapshot(&library(7, 30)), snapshot(&library(8, 30)));
}

#[test]
fn a_seeded_library_covers_the_states_tests_rely_on() {
    let fixture = library(1, 120);
    let library = &fixture.library;

    assert_eq!(library.count(&Scope::All), 120);
    assert!(library.count(&Scope::Inbox) > 0);
    assert!(library.count(&Scope::Inbox) < 120);
    assert!(library.items().iter().any(|item| library.tags_of(item.id).is_empty()));
    let tree: Vec<_> = library
        .folder_tree()
        .iter()
        .map(|row| (row.folder.name.as_str(), row.depth))
        .collect();
    assert_eq!(
        tree,
        [("Default", 0), ("Avatars", 0), ("Kikyo", 1), ("Outfits", 0), ("Shaders", 0)]
    );
    assert!(library.folder_tree().iter().all(|row| row.count > 0));
    assert!(!library.tag_counts().is_empty());
}
