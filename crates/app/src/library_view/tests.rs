use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext};
use snowcase_core::library::{Library, Scope, SortKey};
use snowcase_core::ItemStatus;
use snowcase_testing::fixtures::{self, Fixture};
use snowcase_testing::ui::{click, count, double_click, drop_paths, exists, open, press, type_into};

use super::{Layout, LibraryView};
use crate::library_store::LibraryStore;

fn open_library(
    library: Library,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<LibraryView>) {
    open(cx, |window, cx| {
        let store = cx.new(|_| LibraryStore::new(Ok(library)));
        cx.new(|cx| LibraryView::new(store, window, cx))
    })
}

#[gpui_kit::test]
fn a_new_library_is_empty_with_one_default_folder(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, _) = open_library(library, cx);

    assert!(exists(window, "empty-state", cx));
    assert_eq!(count(window, "asset", cx), 0);
    assert!(exists(window, "scope-folder-1", cx));
    assert!(!exists(window, "scope-folder-2", cx));
}

#[gpui_kit::test]
fn add_files_imports_the_picked_files(cx: &mut TestAppContext) {
    let assets = fixtures::asset_dir();
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, _) = open_library(library, cx);

    click(window, "add-files", cx);
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.directories && options.multiple);
        Some(vec![assets.path().join(fixtures::ASSET_FILES[0])])
    });

    assert_eq!(count(window, "asset", cx), 1);
}

#[gpui_kit::test]
fn dropping_a_folder_imports_it_once_and_counts_failures(cx: &mut TestAppContext) {
    let assets = fixtures::asset_dir();
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, view) = open_library(library, cx);

    drop_paths(window, vec![assets.path().to_path_buf(), assets.path().join("missing.zip")], cx);
    view.read_with(cx, |view, _| assert_eq!(view.last_failures, 1));
    drop_paths(window, vec![assets.path().to_path_buf()], cx);

    assert_eq!(count(window, "asset", cx), fixtures::ASSET_FILES.len());
    view.read_with(cx, |view, _| assert_eq!(view.last_failures, 0));
}

#[gpui_kit::test]
fn scope_filters_and_search_narrow_the_grid(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let all = library.count(&Scope::All);
    let inbox = library.count(&Scope::Inbox);
    let in_library = library.count(&Scope::InLibrary);
    let (tag, tagged) = library
        .tag_counts()
        .first()
        .map(|(tag, count)| (tag.id, *count))
        .unwrap();
    let word = library.items()[0].title.split(' ').next().unwrap().to_owned();
    let found = library.query(&Scope::All, &word, SortKey::Added).len();
    let (window, _) = open_library(library, cx);

    assert_eq!(count(window, "asset", cx), all);
    click(window, "scope-inbox", cx);
    assert_eq!(count(window, "asset", cx), inbox);
    click(window, "scope-in-library", cx);
    assert_eq!(count(window, "asset", cx), in_library);
    click(window, format!("scope-tag-{}", tag.0), cx);
    assert_eq!(count(window, "asset", cx), tagged);
    click(window, "scope-all", cx);
    type_into(window, "search", &word, cx);
    assert_eq!(count(window, "asset", cx), found);
}

#[gpui_kit::test]
fn the_plus_button_adds_a_folder(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, view) = open_library(library, cx);

    click(window, "add-folder", cx);
    click(window, "add-folder", cx);

    view.read_with(cx, |view, cx| {
        let library = view.store.read(cx).library().unwrap();
        let names: Vec<_> = library.folder_tree().iter().map(|row| row.folder.name.clone()).collect();
        assert_eq!(names, ["Default", "New folder 1", "New folder 2"]);
    });
}

#[gpui_kit::test]
fn the_detail_panel_adds_to_library_and_sets_tags_and_folders(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (window, view) = open_library(library, cx);
    click(window, "scope-inbox", cx);
    let id = view.read_with(cx, |view, _| view.rows[0]);

    assert!(!exists(window, "detail", cx));
    click(window, ("asset", 0usize), cx);
    assert!(exists(window, "detail", cx));
    type_into(window, "tag-input", "wishlist", cx);
    press(window, "enter", cx);
    type_into(window, "folder-input", "Gloves", cx);
    press(window, "enter", cx);
    click(window, "add-to-library", cx);

    view.read_with(cx, |view, cx| {
        let library = view.store.read(cx).library().unwrap();
        assert_eq!(library.item(id).unwrap().status, ItemStatus::Confirmed);
        assert!(library.tags_of(id).iter().any(|tag| tag.name == "wishlist"));
        assert!(library.folders_of(id).iter().any(|folder| folder.name == "Gloves"));
        assert!(!view.rows.contains(&id));
    });
}

#[gpui_kit::test]
fn table_layout_lists_the_same_rows(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (window, view) = open_library(library, cx);

    click(window, "layout-table", cx);

    assert!(exists(window, "asset-table", cx));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.layout, Layout::Table);
        let rows: Vec<_> = view.table.read(cx).delegate().rows.iter().map(|row| row.id).collect();
        assert_eq!(rows, view.rows);
    });
}

#[gpui_kit::test]
fn double_clicking_a_card_opens_the_item_page_and_back_returns(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 10);
    let (window, _) = open_library(library, cx);

    double_click(window, ("asset", 0usize), cx);
    assert!(exists(window, "item-page", cx));
    assert!(!exists(window, "filters", cx));

    click(window, "back", cx);
    assert!(!exists(window, "item-page", cx));
    assert!(exists(window, "filters", cx));
}
