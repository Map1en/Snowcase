#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod i18n;
mod library_store;
mod library_view;
mod shell;
mod utils;

use gpui_kit::*;

use snowcase_core::library::Library;

use crate::shell::Shell;

fn main() {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let library_dir = if smoke {
        std::env::temp_dir().join("snowcase-smoke")
    } else {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Snowcase")
    };
    let prefs_path = (!smoke).then(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Snowcase")
            .join("prefs.json")
    });
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            i18n::set_locale(i18n::system_locale(), cx);
            utils::prefs::init(prefs_path, cx);
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
                cx.new(|cx| Shell::new(Library::open(&library_dir), window, cx))
            })
            .expect("failed to open the main window");
        });
}
