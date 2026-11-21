mod booth;
mod files;
mod package_tree;
mod tags;

pub(crate) use booth::BoothStatus;
pub(crate) use files::TreeNode;


use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Snapshot, item_node};
use snowcase_core::{FacetKey, ItemId};

use crate::i18n::tr;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) const DETAIL_WIDTH: f32 = 460.;

impl LibraryView {
    pub(crate) fn render_detail(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let item = library.item(id)?;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let subtitle: Vec<String> = [
            facts.shop.clone(),
            facts.attribute(FacetKey::Version).map(|version| format!("v{version}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let header = h_flex()
            .justify_between()
            .items_start()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child(item.title.clone()))
                    .when(!subtitle.is_empty(), |this| {
                        this.child(div().text_sm().text_color(theme.muted_foreground).child(subtitle.join(" · ")))
                    }),
            )
            .child(
                Button::new("close-detail")
                    .ghost()
                    .compact()
                    .icon(IconName::Close)
                    .tooltip(tr!(cx, "action.close"))
                    .on_click(cx.listener(|this, _, window, cx| this.select(None, window, cx))),
            );
        Some(
            v_flex()
                .id("detail")
                .test_support()
                .w(px(DETAIL_WIDTH))
                .flex_shrink_0()
                .h_full()
                .overflow_y_scroll()
                .gap_5()
                .p_4()
                .border_l_1()
                .border_color(theme.border)
                .child(header)
                .child(cover(library, id, px(200.), theme))
                .child(self.render_attributes(id, library, cx))
                .child(self.render_tags(id, library, cx))
                .child(self.render_folders(id, library, cx))
                .child(self.render_booth(id, library, cx))
                .child(self.render_files(id, library, cx))
                .into_any_element(),
        )
    }

    pub(crate) fn render_attributes(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let facts = library.facts(id).cloned().unwrap_or_default();
        let mut chips = h_flex().flex_wrap().gap_1();
        for (ix, (key, value)) in facts.attributes.iter().cloned().enumerate() {
            let label = match key {
                FacetKey::Version => format!("v{value}"),
                _ => value.clone(),
            };
            chips = chips.child(
                removable(format!("remove-attribute-{ix}"), label, true, cx).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let value = value.clone();
                        this.change(cx, move |library| library.remove_facet(&item_node(id), key, &value));
                    },
                )),
            );
        }
        if let Some(shop) = facts.shop.clone().filter(|_| facts.attribute(FacetKey::Shop).is_none()) {
            chips = chips.child(chip(shop, theme));
        }
        let empty = facts.attributes.is_empty() && facts.shop.is_none();
        section(tr!(cx, "item.attributes"))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "item.attributes_hint")),
            )
            .child(chips)
            .when(empty, |this| {
                this.child(div().text_sm().text_color(theme.muted_foreground).child(tr!(cx, "item.no_attributes")))
            })
    }
}
