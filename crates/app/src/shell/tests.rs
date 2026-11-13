use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size};

use super::{Page, Shell};

#[gpui_kit::test]
fn nav_rail_switches_between_library_tools_and_settings(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1200.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Shell::new(window, cx)),
        )
        .expect("open test window")
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("empty-state").is_some());
        window.click("nav-tools", cx);
        assert!(window.try_find("tools").is_some());
        assert!(window.try_find("empty-state").is_none());
        window.click("nav-settings", cx);
        assert!(window.try_find("settings").is_some());
        window.click("nav-library", cx);
        assert!(window.try_find("empty-state").is_some());
    })
    .unwrap();
    view.read_with(cx, |shell, _| assert_eq!(shell.page, Page::Library));
}
