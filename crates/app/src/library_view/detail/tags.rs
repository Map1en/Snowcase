
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Snapshot, item_node};
use snowcase_core::ItemId;

use crate::i18n::tr;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) const SUGGESTIONS: usize = 6;

impl LibraryView {
    pub(crate) fn add_tag(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        if name.trim().is_empty() {
            return;
        }
        self.change(cx, move |library| library.tag(&item_node(id), &name));
    }

    pub(crate) fn use_tag(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.add_tag(name, cx);
        self.tag_input.update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(crate) fn render_tags(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let own = library.item_tags(id);
        let tags = h_flex().flex_wrap().gap_1().children(own.iter().map(|tag| {
            let tag_id = tag.id;
            removable(format!("remove-tag-{}", tag_id.0), format!("#{}", tag.name), false, cx).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.change(cx, move |library| library.untag(&item_node(id), tag_id));
                }),
            )
        }));
        let draft = self.tag_input.read(cx).value();
        let suggestions: Vec<(String, usize)> = library
            .suggest_tags(&draft)
            .into_iter()
            .filter(|(tag, _)| !own.iter().any(|mine| mine.id == tag.id))
            .take(SUGGESTIONS)
            .map(|(tag, count)| (tag.name.clone(), count))
            .collect();
        let prompt = if draft.trim().is_empty() { tr!(cx, "tag.common") } else { tr!(cx, "tag.similar") };
        let similar = !draft.trim().is_empty() && !suggestions.is_empty();
        section(tr!(cx, "item.my_tags"))
            .child(tags)
            .when(own.is_empty(), |this| {
                this.child(div().text_sm().text_color(theme.muted_foreground).child(tr!(cx, "item.no_tags")))
            })
            .child(Input::new(&self.tag_input).id("tag-input").small())
            .when(!suggestions.is_empty(), |this| {
                this.child(
                    h_flex()
                        .id("tag-suggestions")
                        .test_support()
                        .flex_wrap()
                        .gap_1()
                        .items_center()
                        .p_1()
                        .rounded_md()
                        .when(similar, |this| this.bg(theme.warning.opacity(0.15)))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(prompt))
                        .children(suggestions.into_iter().enumerate().map(|(ix, (name, count))| {
                            Button::new(("suggest-tag", ix))
                                .outline()
                                .xsmall()
                                .label(format!("#{name}  {count}"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.use_tag(name.clone(), window, cx);
                                }))
                        })),
                )
            })
    }

    pub(crate) fn render_folders(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let folders = h_flex().flex_wrap().gap_1().children(library.folders_of(id).into_iter().map(|folder| {
            let folder_id = folder.id;
            removable(format!("remove-folder-{}", folder_id.0), folder.name.clone(), false, cx).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.change(cx, move |library| library.remove_from_folder(id, folder_id));
                }),
            )
        }));
        section(tr!(cx, "item.folders"))
            .child(folders)
            .child(Input::new(&self.folder_input).id("folder-input").small())
    }
}
