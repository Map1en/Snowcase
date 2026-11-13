#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod library_view;
mod settings_view;
mod shell;
mod tools_view;

use gpui_kit::*;

use crate::shell::Shell;

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
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                if smoke {
                    window.on_next_frame(|_, cx| cx.quit());
                }
                cx.new(|cx| Shell::new(window, cx))
            })
            .expect("failed to open the main window");
        });
}
