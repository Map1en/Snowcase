use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::select::Select;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::i18n::tr;
use crate::library_view::*;

impl LibraryView {
    pub(crate) fn render_top_bar(&self, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let active = self.active_filters();
        let select = |field: FilterField, id: &'static str, label: SharedString| {
            self.selects
                .iter()
                .find(|(candidate, _)| *candidate == field)
                .map(|(_, state)| {
                    div()
                        .w(px(170.))
                        .flex_shrink_0()
                        .child(Select::new(state).id(id).placeholder(label).cleanable(true).small())
                })
        };
        let filter_label = if active.is_empty() {
            tr!(cx, "filter.more")
        } else {
            format!("{} · {}", tr!(cx, "filter.more"), active.len()).into()
        };
        let chips = active.into_iter().map(|(field, value)| {
            let (key, id) = match field {
                FilterField::Category => ("filter.category", "active-filter-category"),
                FilterField::Target => ("filter.target", "active-filter-target"),
                FilterField::Shop => ("filter.shop", "active-filter-shop"),
                FilterField::Tag => ("item.tags", "active-filter-tag"),
            };
            let shown = match field {
                FilterField::Tag => self
                    .store
                    .read(cx)
                    .snapshot()
                    .and_then(|library| {
                        library
                            .tag_counts()
                            .into_iter()
                            .find(|(tag, _)| tag.id.0.to_string() == value)
                            .map(|(tag, _)| format!("#{}", tag.name))
                    })
                    .unwrap_or(value),
                _ => value,
            };
            Button::new(id)
                .compact()
                .small()
                .primary()
                .label(format!("{}: {shown}  ×", tr!(cx, key)))
                .tooltip(tr!(cx, "action.remove"))
                .on_click(cx.listener(move |this, _, window, cx| this.clear_filter(field, window, cx)))
        });
        let main = h_flex()
            .gap_2()
            .items_center()
            .px_4()
            .py_2()
            .child(
                Button::new("toggle-nav")
                    .ghost()
                    .compact()
                    .icon(IconName::PanelLeft)
                    .selected(self.nav_open)
                    .tooltip(tr!(cx, "nav.toggle"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav_open = !this.nav_open;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .max_w(px(640.))
                    .child(Input::new(&self.search).id("search").cleanable(true)),
            )
            .child(
                Button::new("more-filters")
                    .outline()
                    .small()
                    .icon(IconName::ChevronDown)
                    .label(filter_label)
                    .selected(self.more_filters)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.more_filters = !this.more_filters;
                        cx.notify();
                    })),
            )
            .child(h_flex().flex_1().min_w_0().gap_1().overflow_hidden().children(chips));
        v_flex()
            .border_b_1()
            .border_color(theme.border)
            .child(main)
            .when(self.more_filters, |this| {
                this.child(
                    h_flex()
                        .id("more-filter-row")
                        .test_support()
                        .flex_wrap()
                        .gap_2()
                        .px_4()
                        .pb_2()
                        .children(select(FilterField::Category, "filter-category", tr!(cx, "filter.category")))
                        .children(select(FilterField::Target, "filter-target", tr!(cx, "filter.target")))
                        .children(select(FilterField::Shop, "filter-shop", tr!(cx, "filter.shop")))
                        .children(select(FilterField::Tag, "filter-tag", tr!(cx, "item.tags"))),
                )
            })
    }
}
