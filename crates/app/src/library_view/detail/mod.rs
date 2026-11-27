mod booth;
mod files;
mod package_tree;
mod tags;

pub(crate) use booth::BoothStatus;
pub(crate) use files::TreeNode;

use std::path::Path;

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Snapshot, item_node};
use snowcase_core::{FacetKey, ItemId, LocationState};

use crate::i18n::tr;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) const DETAIL_WIDTH: f32 = 380.;

pub(crate) const ATTRIBUTE_ROWS: [(FacetKey, &str); 7] = [
    (FacetKey::Category, "filter.category"),
    (FacetKey::Target, "filter.target"),
    (FacetKey::Part, "facet.part"),
    (FacetKey::Color, "facet.color"),
    (FacetKey::Version, "facet.version"),
    (FacetKey::Role, "facet.role"),
    (FacetKey::Shop, "filter.shop"),
];

impl LibraryView {
    pub(crate) fn render_detail(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let item = library.item(id)?;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let subtitle: Vec<String> = [
            facts.shop.clone(),
            facts.attribute(FacetKey::Version).map(|version| format!("v{version}")),
            Some(format_size(facts.size)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let main_path = library.main_file(id).map(|main| main.blob_id).and_then(|blob| {
            library.files(id).into_iter().find(|view| view.file.blob_id == blob).and_then(|view| {
                view.locations
                    .iter()
                    .find(|location| location.state == LocationState::Present)
                    .map(|location| library.location_path(location))
            })
        });
        let title = h_flex()
            .justify_between()
            .items_start()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_lg().font_semibold().line_height(px(24.)).child(item.title.clone()))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(subtitle.join(" · "))),
            )
            .child(
                Button::new("close-detail")
                    .ghost()
                    .compact()
                    .icon(IconName::Close)
                    .tooltip(tr!(cx, "action.close"))
                    .on_click(cx.listener(|this, _, window, cx| this.select(None, window, cx))),
            );
        let actions = h_flex()
            .gap_2()
            .children(main_path.map(|path| {
                Button::new("reveal-main")
                    .outline()
                    .small()
                    .icon(IconName::FolderOpen)
                    .label(tr!(cx, "action.reveal"))
                    .on_click(move |_, _, cx| cx.reveal_path(Path::new(&path)))
            }))
            .children(library.booth_page(id).map(|(url, _)| {
                let url = url.to_owned();
                Button::new("open-booth")
                    .outline()
                    .small()
                    .icon(IconName::ExternalLink)
                    .label(tr!(cx, "booth.open"))
                    .on_click(move |_, _, cx| cx.open_url(&url))
            }));
        let header = v_flex()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(theme.border)
            .child(cover(library, id, px(180.), theme))
            .child(title)
            .child(actions)
            .children(self.render_booth_link(id, library, cx));
        let body = v_flex()
            .id("detail-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_5()
            .p_4()
            .child(self.render_attributes(id, library, cx))
            .child(self.render_tags(id, library, cx))
            .child(self.render_folders(id, library, cx))
            .child(self.render_files(id, library, cx));
        Some(
            v_flex()
                .id("detail")
                .test_support()
                .w(px(DETAIL_WIDTH))
                .flex_shrink_0()
                .h_full()
                .border_l_1()
                .border_color(theme.border)
                .child(header)
                .child(body)
                .into_any_element(),
        )
    }

    pub(crate) fn render_attributes(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let facts = library.facts(id).cloned().unwrap_or_default();
        let mut rows = v_flex().gap_1();
        let mut empty = true;
        for (key, label) in ATTRIBUTE_ROWS {
            let values: Vec<(usize, String)> = facts
                .attributes
                .iter()
                .enumerate()
                .filter(|(_, (candidate, _))| *candidate == key)
                .map(|(ix, (_, value))| (ix, value.clone()))
                .collect();
            let shop = facts.shop.clone().filter(|_| key == FacetKey::Shop && values.is_empty());
            if values.is_empty() && shop.is_none() {
                continue;
            }
            empty = false;
            let mut cell = h_flex().flex_1().min_w_0().flex_wrap().gap_1().items_center();
            for (ix, value) in values {
                let label = if key == FacetKey::Version { format!("v{value}") } else { value.clone() };
                cell = cell.child(removable(format!("remove-attribute-{ix}"), label, true, cx).on_click(
                    cx.listener(move |this, _, _, cx| {
                        let value = value.clone();
                        this.change(cx, move |library| library.remove_facet(&item_node(id), key, &value));
                    }),
                ));
            }
            if let Some(shop) = shop {
                cell = cell.child(div().px_2().text_sm().child(shop));
            }
            rows = rows.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .w(px(56.))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(tr!(cx, label)),
                    )
                    .child(cell),
            );
        }
        section(tr!(cx, "item.attributes")).child(rows).when(empty, |this| {
            this.child(div().text_sm().text_color(theme.muted_foreground).child(tr!(cx, "item.no_attributes")))
        })
    }
}
