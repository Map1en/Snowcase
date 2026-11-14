use gpui_kit::{App, Global, SharedString};
use snowcase_core::StoreError;

pub const LOCALES: [(&str, &str); 5] = [
    ("en", "English"),
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
];

pub struct Locale(pub &'static str);

impl Global for Locale {}

pub fn locale(cx: &App) -> &'static str {
    cx.try_global::<Locale>().map_or("en", |locale| locale.0)
}

pub fn set_locale(locale: &'static str, cx: &mut App) {
    rust_i18n::set_locale(locale);
    cx.set_global(Locale(locale));
    cx.refresh_windows();
}

pub fn system_locale() -> &'static str {
    match_locale(&sys_locale::get_locale().unwrap_or_default())
}

pub fn match_locale(tag: &str) -> &'static str {
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    match parts.next() {
        Some("ja") => "ja",
        Some("ko") => "ko",
        Some("zh") if parts.any(|part| matches!(part, "hant" | "tw" | "hk" | "mo")) => "zh-TW",
        Some("zh") => "zh-CN",
        _ => "en",
    }
}

macro_rules! tr {
    ($cx:expr, $key:expr) => {
        ::gpui_kit::SharedString::from(
            rust_i18n::t!($key, locale = $crate::i18n::locale($cx)).into_owned(),
        )
    };
    ($cx:expr, $key:expr, $($args:tt)+) => {
        ::gpui_kit::SharedString::from(
            rust_i18n::t!($key, locale = $crate::i18n::locale($cx), $($args)+).into_owned(),
        )
    };
}

pub(crate) use tr;

pub fn error_message(error: &StoreError, cx: &App) -> SharedString {
    match error {
        StoreError::Io(detail) => tr!(cx, "error.io", detail = detail),
        StoreError::Open(detail) => tr!(cx, "error.open", detail = detail),
        StoreError::Query(detail) => tr!(cx, "error.query", detail = detail),
        StoreError::Migration(detail) => tr!(cx, "error.migration", detail = detail),
        StoreError::NewerLibrary(_) => tr!(cx, "error.newer_library"),
        StoreError::NonUtf8Path(path) => tr!(cx, "error.non_utf8_path", path = path.display()),
    }
}

#[cfg(test)]
mod tests;
