use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext};
use snowcase_core::FacetKey;
use snowcase_core::library::Library;
use snowcase_core::snapshot::{Filter, Scope, SortKey, item_node};
use snowcase_testing::fixtures::{self, Fixture};
use snowcase_testing::ui::{activate, choose, clear, click, count, drop_paths, exists, open, press, scroll_down, type_into};

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

fn read<T>(view: &Entity<LibraryView>, cx: &mut TestAppContext, read: impl FnOnce(&LibraryView, &snowcase_core::snapshot::Snapshot) -> T) -> T {
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
    read(&view, cx, |_, library| {
        assert_eq!(library.shops(), [("Shop".to_owned(), 3)], "the shop comes from the package root");
    });
    choose(window, "filter-shop", 0, cx);
    assert_eq!(count(window, "asset", cx), 3);
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
        read(view, cx, |_, library| library.item_tags(id).iter().map(|tag| tag.name.clone()).collect())
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
        assert!(library.item_tags(other).iter().any(|tag| tag.name == "black tights"));
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

#[gpui_kit::test]
fn items_from_the_same_package_folder_can_be_merged_from_the_detail(cx: &mut TestAppContext) {
    let assets = tempfile::tempdir().unwrap();
    let outfit = |file: &str| fixtures::unitypackage(&[&format!("Assets/Shop/Outfit/{file}")]);
    std::fs::write(assets.path().join("Outfit_Body.unitypackage"), outfit("Outfit.prefab")).unwrap();
    std::fs::create_dir(assets.path().join("Textures")).unwrap();
    std::fs::write(assets.path().join("Textures/Kisekae_tex.unitypackage"), outfit("Tex.png")).unwrap();
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, _) = open_library(library, cx);
    click(window, "add-root", cx);
    cx.simulate_path_prompt_response(|_| Some(vec![assets.path().to_path_buf()]));
    assert_eq!(count(window, "asset", cx), 2);

    click(window, ("asset", 0usize), cx);
    click(window, ("merge", 0usize), cx);

    assert_eq!(count(window, "asset", cx), 1);
    assert!(exists(window, "detail", cx));
    assert_eq!(count(window, "file", cx), 2);
}

#[gpui_kit::test]
fn opening_the_library_scans_its_folders_and_the_button_scans_again(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(2);
    std::fs::write(assets.path().join("Added.unitypackage"), fixtures::unitypackage(&["Assets/Shop/Added/Added.prefab"])).unwrap();
    let (window, _) = open_library(library, cx);

    assert_eq!(count(window, "asset", cx), 3, "files added while Snowcase was closed show up");
    std::fs::write(assets.path().join("Later.unitypackage"), fixtures::unitypackage(&["Assets/Shop/Later/Later.prefab"])).unwrap();
    click(window, "rescan", cx);
    assert_eq!(count(window, "asset", cx), 4);
}

#[gpui_kit::test]
fn coming_back_to_the_window_rescans_at_most_once_a_minute(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(1);
    let (window, _) = open_library(library, cx);
    let (other, _) = snowcase_testing::ui::open(cx, |_, cx| cx.new(|_| gpui_kit::Empty));
    let come_back = |cx: &mut TestAppContext| {
        activate(other, cx);
        activate(window, cx);
    };
    assert_eq!(count(window, "asset", cx), 1);
    std::fs::write(assets.path().join("New.unitypackage"), fixtures::unitypackage(&["Assets/Shop/New/New.prefab"])).unwrap();

    come_back(cx);
    assert_eq!(count(window, "asset", cx), 1, "scanned less than a minute ago");

    cx.background_executor.advance_clock(std::time::Duration::from_secs(61));
    come_back(cx);
    assert_eq!(count(window, "asset", cx), 2);
}

#[gpui_kit::test]
fn unsupported_archives_are_counted_in_the_status_bar(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(1);
    std::fs::write(assets.path().join("TENSHI_CHOKER.rar"), b"rar").unwrap();
    let (window, view) = open_library(library, cx);

    assert!(exists(window, "unsupported", cx));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.store.read(cx).unsupported(), [assets.path().join("TENSHI_CHOKER.rar").to_string_lossy()]);
    });
}

#[gpui_kit::test]
fn searching_a_part_finds_the_item_whose_package_holds_it(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(2);
    let bikini = fixtures::unitypackage(&["Assets/Shop/Bikini/Bikini_Top.prefab", "Assets/Shop/Bikini/Tights_Black.prefab"]);
    std::fs::write(assets.path().join("Plumery_Bikini.unitypackage"), bikini).unwrap();
    let (window, view) = open_library(library, cx);
    assert_eq!(count(window, "asset", cx), 3);

    type_into(window, "search", "Tights", cx);

    assert_eq!(count(window, "asset", cx), 1);
    assert!(exists(window, ("hit", 0usize), cx));
    read(&view, cx, |view, library| assert_eq!(library.item(view.rows[0]).unwrap().title, "Plumery_Bikini"));
}

fn bikini_library(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<LibraryView>, (tempfile::TempDir, tempfile::TempDir)) {
    let (Fixture { library, dir }, assets) = fixtures::scanned(0);
    let bikini = fixtures::unitypackage(&[
        "Assets/Shop/Bikini/Bikini.prefab",
        "Assets/Shop/Bikini/Parts/Tights_Black.prefab",
        "Assets/Shop/Bikini/Textures/Body.png",
    ]);
    std::fs::write(assets.path().join("Plumery_Bikini.unitypackage"), bikini).unwrap();
    let (window, view) = open_library(library, cx);
    (window, view, (dir, assets))
}

#[gpui_kit::test]
fn a_package_opens_into_a_folder_tree(cx: &mut TestAppContext) {
    let (window, _, _keep) = bikini_library(cx);

    click(window, ("asset", 0usize), cx);
    assert!(!exists(window, ("tree-folder", 0usize), cx));
    click(window, ("package", 0usize), cx);

    assert_eq!(count(window, "tree-folder", cx), 2, "Parts and Textures under the collapsed Assets/Shop/Bikini");
    assert_eq!(count(window, "tree-file", cx), 1, "Bikini.prefab");
    click(window, ("tree-folder", 0usize), cx);
    assert_eq!(count(window, "tree-file", cx), 2);
    click(window, ("package", 0usize), cx);
    assert!(!exists(window, ("tree-folder", 0usize), cx));
}

#[gpui_kit::test]
fn a_search_hit_inside_a_package_opens_the_tree_down_to_it(cx: &mut TestAppContext) {
    let (window, _, _keep) = bikini_library(cx);

    type_into(window, "search", "Tights", cx);
    click(window, ("asset", 0usize), cx);

    assert_eq!(count(window, "tree-file", cx), 2, "Bikini.prefab and the Tights inside the opened Parts folder");
}

#[gpui_kit::test]
fn removing_a_library_folder_asks_first_and_then_drops_its_items(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(2);
    let (window, _) = open_library(library, cx);
    assert_eq!(count(window, "asset", cx), 2);

    click(window, ("remove-root", 1usize), cx);
    cx.simulate_prompt_answer("Cancel");
    assert_eq!(count(window, "asset", cx), 2);

    click(window, ("remove-root", 1usize), cx);
    cx.simulate_prompt_answer("Remove library folder");
    assert!(exists(window, "empty-state", cx));
    assert!(!exists(window, "scope-root-1", cx));
    assert!(assets.path().join("Outfit00.unitypackage").exists());
}

#[gpui_kit::test]
fn a_link_that_is_not_booth_is_refused_without_going_online(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(3, 5);
    let (window, view) = open_library(library, cx);

    click(window, ("asset", 0usize), cx);
    type_into(window, "booth-input", "https://example.com/items/7", cx);
    press(window, "enter", cx);

    assert!(exists(window, "booth-error", cx));
    assert!(!exists(window, "open-booth", cx));
    read(&view, cx, |view, library| assert!(library.booth_page(view.rows[0]).is_none()));
}

#[gpui_kit::test]
fn a_linked_item_shows_its_booth_page_shop_and_cover(cx: &mut TestAppContext) {
    let Fixture { mut library, dir: _dir } = fixtures::library(3, 5);
    let id = library.items()[0].id;
    let booth = snowcase_core::library::StorePage {
        external_id: "7".into(),
        name: "Sample Outfit".into(),
        shop: "Sample Shop".into(),
        url: "https://shop.booth.pm/items/7".into(),
    };
    let picture = snowcase_core::library::Picture { bytes: b"not really a jpeg".to_vec(), mime: "image/jpeg".into() };
    library.link_booth(id, &booth, Some(&picture)).unwrap();
    let (window, view) = open_library(library, cx);
    let ix = read(&view, cx, |view, _| view.rows.iter().position(|row| *row == id).unwrap());

    click(window, ("asset", ix), cx);

    assert!(exists(window, "open-booth", cx));
    read(&view, cx, |_, library| {
        assert_eq!(library.booth_page(id).map(|(_, title)| title), Some("Sample Outfit"));
        assert_eq!(library.shops(), [("Sample Shop".to_owned(), 1)]);
        assert!(library.cover_path(id).is_some());
    });
}

#[gpui_kit::test]
fn scrolling_the_grid_moves_past_the_first_rows(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::library(7, 300);
    let (window, _) = open_library(library, cx);

    assert!(exists(window, ("asset", 0usize), cx));
    scroll_down(window, ("asset", 0usize), 3000., cx);

    assert!(!exists(window, ("asset", 0usize), cx));
    assert!((60usize..300).any(|ix| exists(window, ("asset", ix), cx)));
}
