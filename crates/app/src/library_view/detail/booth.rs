
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use snowcase_booth as booth;
use snowcase_core::snapshot::Snapshot;
use snowcase_core::ItemId;

use crate::i18n::tr;
use crate::library_view::*;
use crate::library_view::widgets::*;

pub(crate) enum BoothStatus {
    Fetching,
    Failed(String),
}

impl LibraryView {
    pub(crate) fn link_booth(&mut self, id: ItemId, text: &str, cx: &mut Context<Self>) {
        let Some(booth_id) = booth::item_id(text) else {
            self.booth_status = Some(BoothStatus::Failed(tr!(cx, "booth.not_a_link").to_string()));
            cx.notify();
            return;
        };
        self.booth_status = Some(BoothStatus::Fetching);
        cx.notify();
        let linked = self.store.update(cx, |store, cx| store.link_booth(id, booth_id, cx));
        cx.spawn(async move |this, cx| {
            let linked = linked.await;
            this.update(cx, |this, cx| {
                this.booth_status = linked.err().map(BoothStatus::Failed);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_booth(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let page = library.booth_page(id).map(|(url, title)| (url.to_owned(), title.to_owned()));
        section(tr!(cx, "booth.title"))
            .children(page.map(|(url, title)| {
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(title))
                    .child(
                        Button::new("open-booth")
                            .ghost()
                            .small()
                            .icon(IconName::ExternalLink)
                            .label(tr!(cx, "booth.open"))
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
            }))
            .child(Input::new(&self.booth_input).id("booth-input").small())
            .children(self.booth_status.as_ref().map(|status| match status {
                BoothStatus::Fetching => div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "booth.fetching"))
                    .into_any_element(),
                BoothStatus::Failed(error) => div()
                    .id("booth-error")
                    .test_support()
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error.clone())
                    .into_any_element(),
            }))
    }
}
