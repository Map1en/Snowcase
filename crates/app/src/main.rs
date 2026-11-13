#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod library_view;

use gpui_kit::*;

use crate::library_view::LibraryView;

fn main() {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Snowcase".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.), px(720.)),
                    cx,
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                if smoke {
                    window.on_next_frame(|_, cx| cx.quit());
                }
                cx.new(|_| LibraryView::default())
            })
            .expect("failed to open the main window");
        });
}
