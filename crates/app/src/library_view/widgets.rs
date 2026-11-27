
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Snapshot, shown_name};
use snowcase_core::ItemId;

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

pub(crate) fn placeholder_color(seed: &str) -> Hsla {
    let hash = seed.chars().fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32));
    hsla((hash % 360) as f32 / 360., 0.38, 0.55, 1.)
}

pub(crate) fn initials(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).take(2).flat_map(char::to_uppercase).collect()
}

pub(crate) fn main_extension(library: &Snapshot, id: ItemId) -> Option<String> {
    let name = shown_name(library.main_file(id)?);
    name.rsplit_once('.').map(|(_, extension)| extension.to_uppercase())
}

pub(crate) fn placeholder_seed(library: &Snapshot, id: ItemId) -> String {
    let shop = library.facts(id).and_then(|facts| facts.shop.clone());
    shop.or_else(|| library.item(id).map(|item| item.title.clone())).unwrap_or_default()
}

pub(crate) fn cover(library: &Snapshot, id: ItemId, height: Pixels, theme: &Theme) -> Div {
    let frame = div()
        .relative()
        .w_full()
        .h(height)
        .flex_shrink_0()
        .rounded_md()
        .overflow_hidden();
    if let Some(path) = library.cover_path(id) {
        return frame.bg(theme.muted).child(img(path).size_full().object_fit(ObjectFit::Cover));
    }
    let title = library.item(id).map(|item| initials(&item.title)).unwrap_or_default();
    let extension = main_extension(library, id);
    let big = height > px(160.);
    frame
        .flex()
        .items_center()
        .justify_center()
        .bg(placeholder_color(&placeholder_seed(library, id)))
        .child(
            div()
                .when(big, |this| this.text_3xl())
                .when(!big, |this| this.text_2xl())
                .font_bold()
                .text_color(gpui_kit::white().opacity(0.85))
                .child(title),
        )
        .children(extension.map(|extension| {
            div()
                .absolute()
                .left_1p5()
                .bottom_1p5()
                .px_1()
                .rounded_sm()
                .text_xs()
                .font_semibold()
                .bg(gpui_kit::black().opacity(0.4))
                .text_color(gpui_kit::white())
                .child(extension)
        }))
}
