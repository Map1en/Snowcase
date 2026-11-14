use gpui_kit::{AppContext, TestAppContext};
use snowcase_testing::fixtures::{self, Fixture};
use snowcase_testing::ui::{click, exists, open};

use super::{Page, Shell};

#[gpui_kit::test]
fn nav_rail_switches_between_library_tools_and_settings(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, view) = open(cx, |window, cx| cx.new(|cx| Shell::new(Ok(library), window, cx)));

    assert!(exists(window, "filters", cx));
    click(window, "nav-tools", cx);
    assert!(exists(window, "tools", cx));
    assert!(!exists(window, "filters", cx));
    click(window, "nav-settings", cx);
    assert!(exists(window, "settings", cx));
    click(window, "nav-library", cx);
    assert!(exists(window, "filters", cx));
    view.read_with(cx, |shell, _| assert_eq!(shell.page, Page::Library));
}
