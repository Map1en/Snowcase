
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use snowcase_core::snapshot::{Snapshot, shown_name};
use snowcase_core::{FileKind, ItemId};

use crate::i18n::tr;

pub(crate) fn section(label: SharedString) -> Div {
    v_flex()
        .gap_2()
        .child(div().text_xs().font_semibold().opacity(0.6).child(label))
}

pub(crate) fn removable(id: String, label: impl Into<SharedString>, light: bool, cx: &App) -> Button {
    let button = Button::new(SharedString::from(id))
        .compact()
        .small()
        .label(format!("{}  ×", label.into()))
        .tooltip(tr!(cx, "action.remove"));
    if light { button.ghost() } else { button.outline() }
}

pub(crate) fn chip(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_sm()
        .bg(theme.muted)
        .text_color(theme.muted_foreground)
        .child(text.into())
}

pub(crate) fn badge(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_1p5()
        .rounded_md()
        .text_xs()
        .bg(theme.accent)
        .text_color(theme.accent_foreground)
        .child(text.into())
}

pub(crate) fn hint(text: SharedString, theme: &Theme) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .bg(theme.muted)
        .text_color(theme.muted_foreground)
        .child(text)
}

pub(crate) fn format_size(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("B"))
    }
}

pub(crate) fn cover(library: &Snapshot, id: ItemId, height: Pixels, theme: &Theme) -> Div {
    if let Some(path) = library.cover_path(id) {
        return div()
            .w_full()
            .h(height)
            .flex_shrink_0()
            .rounded_md()
            .overflow_hidden()
            .bg(theme.muted)
            .child(img(path).size_full().object_fit(ObjectFit::Cover));
    }
    let file = library.main_file(id);
    let file_name = file.map(shown_name);
    let kind = file
        .and_then(|file| library.files(id).into_iter().find(|view| view.file.blob_id == file.blob_id))
        .map(|view| FileKind::from(view.blob.kind));
    let icon = match kind {
        Some(FileKind::UnityPackage | FileKind::Model) => IconName::Frame,
        Some(FileKind::Archive) => IconName::FolderClosed,
        Some(FileKind::Image) => IconName::GalleryVerticalEnd,
        Some(FileKind::Other) | None => IconName::File,
    };
    let extension = file_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_uppercase());
    v_flex()
        .w_full()
        .h(height)
        .flex_shrink_0()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.muted)
        .items_center()
        .justify_center()
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size(px(28.)))
        .children(extension.map(|extension| div().text_xs().font_semibold().child(extension)))
        .children(file_name.map(|name| {
            div()
                .w_full()
                .text_xs()
                .text_center()
                .line_clamp(2)
                .child(name.to_owned())
        }))
}
