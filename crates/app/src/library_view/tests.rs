use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext};
use snowcase_core::FacetKey;
use snowcase_core::library::{Filter, Library, Scope, SortKey, item_node};
use snowcase_testing::fixtures::{self, Fixture};
use snowcase_testing::ui::{choose, clear, click, count, drop_paths, exists, open, press, type_into};

use super::LibraryView;
use crate::i18n;
use crate::library_store::LibraryStore;

fn open_library(
    library: Library,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<LibraryView>) {
    open(cx, |window, cx| {
        let store = cx.new(|cx| LibraryStore::new(Ok(library), cx));
        cx.new(|cx| LibraryView::new(store, window, cx))
    })
}

fn read<T>(view: &Entity<LibraryView>, cx: &mut TestAppContext, read: impl FnOnce(&LibraryView, &snowcase_core::library::Snapshot) -> T) -> T {
    view.read_with(cx, |view, cx| read(view, view.store.read(cx).snapshot().unwrap()))
}

#[gpui_kit::test]
fn a_new_library_is_empty_with_one_default_folder(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, _) = open_library(library, cx);

    assert!(exists(window, "empty-state", cx));
    assert_eq!(count(window, "asset", cx), 0);
    assert!(exists(window, "scope-folder-1", cx));
    assert!(!exists(window, "scope-folder-2", cx));
    assert!(exists(window, "add-root", cx));
}

#[gpui_kit::test]
fn adding_a_library_folder_scans_it_and_lists_it_on_the_left(cx: &mut TestAppContext) {
    let assets = fixtures::packages(3);
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, view) = open_library(library, cx);

    click(window, "add-root", cx);
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files && !options.multiple);
        Some(vec![assets.path().to_path_buf()])
    });

    assert_eq!(count(window, "asset", cx), 3);
    assert!(exists(window, "scope-root-1", cx));
    click(window, "scope-folder-1", cx);
    assert!(exists(window, "no-results", cx));
    click(window, "scope-root-1", cx);
    assert_eq!(count(window, "asset", cx), 3);
    read(&view, cx, |_, library| {
        let fingerprinted = library.files(library.items()[0].id)[0].blob.fingerprint.is_some();
        assert!(fingerprinted, "adding a folder starts the background work");
    });
    click(window, ("asset", 0usize), cx);
    assert!(exists(window, ("package", 0usize), cx), "the package was read in the background");
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
fn the_type_and_fits_selects_narrow_the_grid_and_clear_again(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 20);
    let all = library.count(&Scope::All);
    let (category, in_category) = library.values(FacetKey::Category)[0].clone();
    let (_, fitting) = library.values(FacetKey::Target)[0].clone();
    let (window, view) = open_library(library, cx);

    choose(window, "filter-category", 0, cx);
    assert_eq!(count(window, "asset", cx), in_category);
    read(&view, cx, |view, _| assert_eq!(view.filter.category.as_deref(), Some(category.as_str())));

    clear(window, "filter-category", cx);
    assert_eq!(count(window, "asset", cx), all);
    choose(window, "filter-target", 0, cx);
    assert_eq!(count(window, "asset", cx), fitting);
}

#[gpui_kit::test]
fn more_filters_reveal_the_tag_select(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (_, tagged) = library.tag_counts()[0];
    let (window, _) = open_library(library, cx);

    assert!(!exists(window, "filter-tag", cx));
    click(window, "more-filters", cx);
    choose(window, "filter-tag", 0, cx);

    assert_eq!(count(window, "asset", cx), tagged);
}

#[gpui_kit::test]
fn searching_a_file_name_shows_where_it_matched(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 20);
    let found = library.query(&Filter::search(".unitypackage"), SortKey::Added).len();
    let (window, _) = open_library(library, cx);

    assert!(!exists(window, ("hit", 0usize), cx));
    type_into(window, "search", ".unitypackage", cx);

    assert_eq!(count(window, "asset", cx), found);
    assert_eq!(count(window, "hit", cx), found);
}

#[gpui_kit::test]
fn the_sidebar_folds_away_and_comes_back(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 10);
    let (window, _) = open_library(library, cx);

    click(window, "toggle-nav", cx);
    assert!(!exists(window, "filters", cx));
    click(window, "toggle-nav", cx);
    assert!(exists(window, "filters", cx));
}

#[gpui_kit::test]
fn the_plus_button_adds_a_folder(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, view) = open_library(library, cx);

    click(window, "add-folder", cx);
    click(window, "add-folder", cx);

    read(&view, cx, |_, library| {
        let names: Vec<_> = library.folder_tree().iter().map(|row| row.folder.name.clone()).collect();
        assert_eq!(names, ["Default", "New folder 1", "New folder 2"]);
    });
}

#[gpui_kit::test]
fn the_detail_drawer_adds_tags_by_enter_comma_or_suggestion(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (window, view) = open_library(library, cx);
    let id = read(&view, cx, |view, _| view.rows[0]);
    let names = |view: &Entity<LibraryView>, cx: &mut TestAppContext| -> Vec<String> {
        read(view, cx, |_, library| library.tags_of(id).iter().map(|tag| tag.name.clone()).collect())
    };
    let before = names(&view, cx);

    assert!(!exists(window, "detail", cx));
    click(window, ("asset", 0usize), cx);
    assert!(exists(window, "detail", cx));
    type_into(window, "tag-input", "wishlist", cx);
    press(window, "enter", cx);
    type_into(window, "tag-input", "black tights，winter, ", cx);
    let added = names(&view, cx);
    for tag in ["wishlist", "black tights", "winter"] {
        assert!(added.contains(&tag.to_owned()), "{tag} in {added:?}");
    }

    let other = read(&view, cx, |view, _| view.rows[1]);
    click(window, ("asset", 1usize), cx);
    type_into(window, "tag-input", "Black", cx);
    assert!(exists(window, "tag-suggestions", cx));
    click(window, ("suggest-tag", 0usize), cx);
    read(&view, cx, |_, library| {
        assert!(library.tags_of(other).iter().any(|tag| tag.name == "black tights"));
    });

    click(window, "close-detail", cx);
    assert!(!exists(window, "detail", cx));
    assert!(before.len() < names(&view, cx).len());
}

#[gpui_kit::test]
fn removing_a_recognized_attribute_takes_it_out_of_the_filters(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (window, view) = open_library(library, cx);
    let id = read(&view, cx, |view, library| {
        *view
            .rows
            .iter()
            .find(|id| library.facets(&item_node(**id), FacetKey::Category).len() == 1)
            .unwrap()
    });
    let ix = read(&view, cx, |view, _| view.rows.iter().position(|row| *row == id).unwrap());

    click(window, ("asset", ix), cx);
    click(window, "remove-attribute-0", cx);

    read(&view, cx, |_, library| {
        assert!(library.facets(&item_node(id), FacetKey::Category).is_empty());
    });
}

#[gpui_kit::test]
fn the_detail_drawer_puts_the_item_in_a_folder(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 40);
    let (window, view) = open_library(library, cx);
    let id = read(&view, cx, |view, _| view.rows[0]);

    click(window, ("asset", 0usize), cx);
    type_into(window, "folder-input", "Gloves", cx);
    press(window, "enter", cx);

    read(&view, cx, |view, library| {
        assert!(library.folders_of(id).iter().any(|folder| folder.name == "Gloves"));
        assert!(view.rows.contains(&id));
    });
}

#[gpui_kit::test]
fn switching_language_relabels_the_inputs(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 10);
    let (window, view) = open_library(library, cx);
    let placeholder = |view: &LibraryView, cx: &gpui_kit::App| view.search.read(cx).presentation().placeholder().to_string();
    view.read_with(cx, |view, cx| assert_eq!(placeholder(view, cx), "Search titles, tags and files"));

    cx.update(|cx| i18n::set_locale("ja", cx));

    assert!(exists(window, "filters", cx));
    view.read_with(cx, |view, cx| assert_eq!(placeholder(view, cx), "タイトル・タグ・ファイルを検索"));
}

#[gpui_kit::test]
fn the_top_bar_stays_one_row_so_nothing_covers_the_sidebar_or_the_cards(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 10);
    let (window, view) = open_library(library, cx);

    click(window, "scope-folder-2", cx);
    click(window, "scope-all", cx);
    click(window, ("asset", 0usize), cx);

    assert!(exists(window, "detail", cx));
    view.read_with(cx, |view, _| assert_eq!(view.filter.scope, Scope::All));
}
