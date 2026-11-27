use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Hit, Snapshot, SortKey};
use snowcase_core::{FacetKey, ItemId};

use crate::i18n::tr;
use crate::library_store::LibraryStore;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) const CARD_WIDTH: f32 = 172.;

pub(crate) const CARD_HEIGHT: f32 = 244.;

pub(crate) const GRID_GAP: f32 = 12.;

pub(crate) const GRID_PADDING: f32 = 16.;

pub(crate) const ROW_HEIGHT: f32 = 36.;

impl LibraryView {
    pub(crate) fn render_results_bar(&self, total: usize, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let sort = self.sort;
        let descending = matches!(sort, SortKey::Added | SortKey::Size) != self.reversed;
        h_flex()
            .gap_2()
            .items_center()
            .px(px(GRID_PADDING))
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(
                div()
                    .id("result-count")
                    .flex_1()
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
                        this.reversed = false;
                        this.refresh(cx);
                    })),
            )
            .child(
                Button::new("sort-direction")
                    .ghost()
                    .compact()
                    .icon(if descending { IconName::SortDescending } else { IconName::SortAscending })
                    .tooltip(tr!(cx, "sort.direction"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reversed = !this.reversed;
                        this.refresh(cx);
                    })),
            )
            .child(
                ButtonGroup::new("view")
                    .outline()
                    .compact()
                    .children([
                        Button::new("view-grid")
                            .icon(IconName::LayoutDashboard)
                            .tooltip(tr!(cx, "view.grid"))
                            .selected(!self.list_view),
                        Button::new("view-list")
                            .icon(IconName::Menu)
                            .tooltip(tr!(cx, "view.list"))
                            .selected(self.list_view),
                    ])
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        this.list_view = selected.first() == Some(&1);
                        cx.notify();
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
            .overflow_hidden()
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
        let selected = self.selected == Some(id);
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
            .border_color(if selected { theme.primary } else { transparent_black() })
            .when(selected, |this| this.bg(theme.primary.opacity(0.06)))
            .when(!selected, |this| this.hover(|this| this.bg(theme.list_hover)))
            .child(
                div()
                    .relative()
                    .child(cover(library, id, px(150.), theme))
                    .when(library.booth_page(id).is_none(), |this| {
                        this.child(
                            div()
                                .id(("unlinked", ix))
                                .test_support()
                                .absolute()
                                .top_1p5()
                                .right_1p5()
                                .size_2()
                                .rounded_full()
                                .border_1()
                                .border_color(gpui_kit::white())
                                .bg(theme.warning),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(40.))
                    .text_sm()
                    .line_height(px(20.))
                    .font_medium()
                    .line_clamp(2)
                    .child(item.title.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .truncate()
                    .child(card_meta(library, id)),
            )
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

    pub(crate) fn render_list(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let sizes = Rc::new(vec![size(px(0.), px(ROW_HEIGHT)); self.rows.len()]);
        let header = list_columns(
            h_flex()
                .h(px(ROW_HEIGHT - 8.))
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .border_b_1()
                .border_color(theme.border),
            [
                tr!(cx, "item.name"),
                tr!(cx, "filter.shop"),
                tr!(cx, "facet.version"),
                tr!(cx, "item.type"),
                tr!(cx, "item.size"),
                "BOOTH".into(),
            ],
        );
        v_flex()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        v_virtual_list(cx.entity(), "asset-list", sizes, move |this, range, _, cx| {
                            let store = this.store.read(cx);
                            let Some(library) = store.snapshot() else {
                                return Vec::new();
                            };
                            range.map(|ix| this.render_row(ix, library, cx)).collect()
                        })
                        .track_scroll(&self.grid_scroll)
                        .size_full(),
                    )
                    .child(Scrollbar::vertical(&self.grid_scroll)),
            )
            .into_any_element()
    }

    pub(crate) fn render_row(&self, ix: usize, library: &Snapshot, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(item) = self.rows.get(ix).and_then(|id| library.item(*id)) else {
            return div().into_any_element();
        };
        let id = item.id;
        let selected = self.selected == Some(id);
        let facts = library.facts(id);
        let shop = facts.and_then(|facts| facts.shop.clone());
        let version = facts.and_then(|facts| facts.attribute(FacetKey::Version));
        let size = facts.map_or(0, |facts| facts.size);
        let dash = || SharedString::from("—");
        let name = h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .size_6()
                    .flex_shrink_0()
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .font_bold()
                    .text_color(gpui_kit::white().opacity(0.85))
                    .bg(placeholder_color(&placeholder_seed(library, id)))
                    .child(initials(&item.title).chars().next().map(String::from).unwrap_or_default()),
            )
            .child(div().truncate().child(item.title.clone()));
        let cells: [AnyElement; 6] = [
            name.into_any_element(),
            shop.map_or_else(dash, SharedString::from).into_any_element(),
            version.map_or_else(dash, |version| format!("v{version}").into()).into_any_element(),
            main_extension(library, id).map_or_else(dash, SharedString::from).into_any_element(),
            format_size(size).into_any_element(),
            if library.booth_page(id).is_some() {
                SharedString::from("✓").into_any_element()
            } else {
                div().text_color(theme.warning).child(tr!(cx, "scope.unlinked")).into_any_element()
            },
        ];
        list_columns(
            h_flex()
                .id(("asset", ix))
                .test_support()
                .h(px(ROW_HEIGHT))
                .text_sm()
                .border_b_1()
                .border_color(theme.border.opacity(0.5))
                .cursor_pointer()
                .when(selected, |this| this.bg(theme.primary.opacity(0.08)))
                .when(!selected, |this| this.hover(|this| this.bg(theme.list_hover)))
                .on_click(cx.listener(move |this, _, window, cx| this.select(Some(id), window, cx))),
            cells,
        )
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
            Some(_) if self.list_view => self.render_list(cx),
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

pub(crate) fn card_meta(library: &Snapshot, id: ItemId) -> String {
    let facts = library.facts(id);
    let shop = facts.and_then(|facts| facts.shop.clone());
    let size = format_size(facts.map_or(0, |facts| facts.size));
    shop.into_iter().chain([size]).collect::<Vec<_>>().join(" · ")
}

pub(crate) fn list_columns<E: ParentElement + Styled>(row: E, cells: [impl IntoElement; 6]) -> E {
    let widths = [None, Some(140.), Some(70.), Some(110.), Some(80.), Some(110.)];
    let mut row = row.w_full().px(px(GRID_PADDING)).gap_3().items_center();
    for (cell, width) in cells.into_iter().zip(widths) {
        let column = div().min_w_0().truncate().child(cell);
        row = row.child(match width {
            Some(width) => column.w(px(width)),
            None => column.flex_1().min_w(px(160.)),
        });
    }
    row
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
