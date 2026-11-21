
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
        let select = |field: FilterField, id: &'static str, label: SharedString| {
            self.selects
                .iter()
                .find(|(candidate, _)| *candidate == field)
                .map(|(_, state)| {
                    div()
                        .w(px(150.))
                        .flex_shrink_0()
                        .child(Select::new(state).id(id).placeholder(label).cleanable(true).small())
                })
        };
        let main = h_flex()
            .gap_2()
            .items_center()
            .flex_wrap()
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
                    .child(Input::new(&self.search).id("search").cleanable(true)),
            )
            .children(select(FilterField::Category, "filter-category", tr!(cx, "filter.category")))
            .children(select(FilterField::Target, "filter-target", tr!(cx, "filter.target")))
            .children(select(FilterField::Shop, "filter-shop", tr!(cx, "filter.shop")))
            .child(
                Button::new("more-filters")
                    .outline()
                    .small()
                    .label(tr!(cx, "filter.more"))
                    .selected(self.more_filters)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.more_filters = !this.more_filters;
                        cx.notify();
                    })),
            );
        v_flex()
            .border_b_1()
            .border_color(theme.border)
            .child(main)
            .when(self.more_filters, |this| {
                this.child(
                    h_flex()
                        .id("more-filter-row")
                        .test_support()
                        .gap_2()
                        .px_4()
                        .pb_2()
                        .children(select(FilterField::Tag, "filter-tag", tr!(cx, "item.tags"))),
                )
            })
    }
}
