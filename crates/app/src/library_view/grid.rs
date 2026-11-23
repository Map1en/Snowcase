use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Hit, Snapshot, SortKey};
use snowcase_core::FacetKey;

use crate::i18n::tr;
use crate::library_store::LibraryStore;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) const CARD_WIDTH: f32 = 172.;

pub(crate) const CARD_HEIGHT: f32 = 236.;

pub(crate) const GRID_GAP: f32 = 12.;

pub(crate) const GRID_PADDING: f32 = 16.;

impl LibraryView {
    pub(crate) fn render_results_bar(&self, total: usize, cx: &Context<Self>) -> Div {
        let sort = self.sort;
        h_flex()
            .justify_between()
            .items_center()
            .px(px(GRID_PADDING))
            .pt_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .id("result-count")
                    .child(tr!(cx, "status.count", shown = self.rows.len(), total = total)),
            )
            .child(
                ButtonGroup::new("sort")
                    .ghost()
                    .compact()
                    .children([
                        Button::new("sort-added")
                            .label(tr!(cx, "sort.newest"))
                            .selected(sort == SortKey::Added),
                        Button::new("sort-name")
                            .label(tr!(cx, "item.name"))
                            .selected(sort == SortKey::Name),
                        Button::new("sort-size")
                            .label(tr!(cx, "item.size"))
                            .selected(sort == SortKey::Size),
                    ])
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        this.sort = match selected.first() {
                            Some(1) => SortKey::Name,
                            Some(2) => SortKey::Size,
                            _ => SortKey::Added,
                        };
                        this.refresh(cx);
                    })),
            )
    }

    pub(crate) fn render_grid(&self, cx: &Context<Self>) -> AnyElement {
        let usable = f32::from(self.grid_width) - GRID_PADDING * 2. + GRID_GAP;
        let columns = ((usable / (CARD_WIDTH + GRID_GAP)).floor() as usize).max(1);
        let sizes = Rc::new(vec![
            size(self.grid_width, px(CARD_HEIGHT + GRID_GAP));
            self.rows.len().div_ceil(columns)
        ]);
        let view = cx.entity().downgrade();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |this, cx| {
                            if this.grid_width != bounds.size.width {
                                this.grid_width = bounds.size.width;
                                cx.notify();
                            }
                        })
                        .ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                v_virtual_list(cx.entity(), "asset-grid", sizes, move |this, range, _, cx| {
                    let store = this.store.read(cx);
                    let Some(library) = store.snapshot() else {
                        return Vec::new();
                    };
                    range
                        .map(|row| {
                            let start = row * columns;
                            let end = (start + columns).min(this.rows.len());
                            h_flex()
                                .gap(px(GRID_GAP))
                                .px(px(GRID_PADDING))
                                .pt(px(GRID_GAP))
                                .children((start..end).map(|ix| this.render_card(ix, library, cx)))
                        })
                        .collect()
                })
                .track_scroll(&self.grid_scroll)
                .size_full(),
            )
            .child(Scrollbar::vertical(&self.grid_scroll))
            .into_any_element()
    }

    pub(crate) fn render_card(&self, ix: usize, library: &Snapshot, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(item) = self.rows.get(ix).and_then(|id| library.item(*id)) else {
            return div().into_any_element();
        };
        let id = item.id;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let meta: Vec<String> = [facts.attribute(FacetKey::Category).map(str::to_owned), facts.shop.clone()]
            .into_iter()
            .flatten()
            .collect();
        let meta = if meta.is_empty() { format_size(facts.size) } else { meta.join(" · ") };
        let hit = library.hit(id, &self.filter.search).map(|hit| hit_label(&hit, cx));
        v_flex()
            .id(("asset", ix))
            .test_support()
            .w(px(CARD_WIDTH))
            .h(px(CARD_HEIGHT))
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_2()
            .cursor_pointer()
            .border_color(if self.selected == Some(id) {
                theme.ring
            } else {
                transparent_black()
            })
            .hover(|this| this.bg(theme.list_hover))
            .child(cover(library, id, px(150.), theme))
            .child(div().text_sm().font_medium().line_clamp(2).child(item.title.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(meta))
            .children(hit.map(|hit| {
                div()
                    .id(("hit", ix))
                    .test_support()
                    .px_1()
                    .rounded_sm()
                    .text_xs()
                    .bg(theme.accent)
                    .text_color(theme.accent_foreground)
                    .truncate()
                    .child(hit)
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.select(Some(id), window, cx)))
            .into_any_element()
    }

    pub(crate) fn render_main(&self, store: &LibraryStore, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let total = store.snapshot().map_or(0, |library| library.items().len());
        let body = match store.snapshot() {
            None => centered("library-error", tr!(cx, "empty.no_library"), muted),
            Some(library) if library.items().is_empty() => {
                centered("empty-state", tr!(cx, "empty.library"), muted)
            }
            Some(_) if self.rows.is_empty() => {
                centered("no-results", tr!(cx, "empty.no_results"), muted)
            }
            Some(_) => self.render_grid(cx),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(total > 0, |this| this.child(self.render_results_bar(total, cx)))
            .child(body)
    }
}

pub(crate) fn hit_label(hit: &Hit, cx: &App) -> SharedString {
    match hit {
        Hit::File(name) | Hit::Attribute(name) => tr!(cx, "hit.match", name = name),
        Hit::Package { package, path, .. } => {
            tr!(cx, "hit.package", name = path.rsplit('/').next().unwrap_or(path), package = package)
        }
        Hit::Tag { name, file: None } => tr!(cx, "hit.tag", name = name),
        Hit::Tag { name, file: Some(file) } => tr!(cx, "hit.tag_in", name = name, file = file),
    }
}

pub(crate) fn centered(id: &'static str, text: SharedString, color: Hsla) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color)
        .child(text)
        .into_any_element()
}
