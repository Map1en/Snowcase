use gpui_kit::test::TestWindowExt;
use std::path::PathBuf;

use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, ElementId, Entity, ExternalPaths, FileDropEvent,
    Point, Render, TestAppContext, VisualTestContext, Window, WindowBounds, WindowOptions, point,
    px, size,
};

pub fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> (AnyWindowHandle, Entity<V>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1400.), px(1200.)),
                })),
                ..Default::default()
            },
            cx,
            build,
        )
        .expect("open test window")
    })
}

pub fn settle(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
}

pub fn exists(window: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) -> bool {
    settle(window, cx);
    let id = id.into();
    cx.update_window(window, |_, window, _| window.try_find(id).is_some())
        .unwrap()
}

pub fn count(window: AnyWindowHandle, name: &'static str, cx: &mut TestAppContext) -> usize {
    settle(window, cx);
    cx.update_window(window, |_, window, _| {
        (0usize..)
            .take_while(|ix| window.try_find((name, *ix)).is_some())
            .count()
    })
    .unwrap()
}

pub fn click(window: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    settle(window, cx);
    let id = id.into();
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .unwrap();
    settle(window, cx);
}

pub fn double_click(window: AnyWindowHandle, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    settle(window, cx);
    let id = id.into();
    cx.update_window(window, |_, window, cx| window.double_click(id, cx))
        .unwrap();
    settle(window, cx);
}

pub fn type_into(
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
    text: &str,
    cx: &mut TestAppContext,
) {
    click(window, id, cx);
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .unwrap();
    settle(window, cx);
}

pub fn drop_paths(window: AnyWindowHandle, paths: Vec<PathBuf>, cx: &mut TestAppContext) {
    let position = point(px(700.), px(600.));
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(FileDropEvent::Entered {
        position,
        paths: ExternalPaths(paths.into_iter().collect()),
    });
    vcx.simulate_event(FileDropEvent::Submit { position });
    settle(window, cx);
}

pub fn press(window: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .unwrap();
    settle(window, cx);
}
