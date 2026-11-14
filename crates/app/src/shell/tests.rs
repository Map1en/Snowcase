use gpui_kit::{AppContext, TestAppContext};
use snowcase_testing::fixtures::{self, Fixture};
use snowcase_testing::ui::{click, exists, open};

use super::{Page, Shell};
use crate::i18n;

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

#[gpui_kit::test]
fn choosing_a_language_in_settings_switches_it_immediately(cx: &mut TestAppContext) {
    let Fixture { library, dir: _dir } = fixtures::empty();
    let (window, _) = open(cx, |window, cx| cx.new(|cx| Shell::new(Ok(library), window, cx)));

    assert_eq!(cx.update(|cx| i18n::locale(cx)), "en");
    click(window, "nav-settings", cx);
    click(window, "locale-ja", cx);
    assert_eq!(cx.update(|cx| i18n::locale(cx)), "ja");
    click(window, "locale-zh-TW", cx);
    assert_eq!(cx.update(|cx| i18n::locale(cx)), "zh-TW");
    assert!(exists(window, "settings", cx));
}
