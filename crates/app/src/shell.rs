use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::StoreError;
use snowcase_core::library::Library;

use crate::library_store::LibraryStore;
use crate::library_view::LibraryView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Library,
    Tools,
    Settings,
}

pub struct Shell {
    page: Page,
    library_view: Entity<LibraryView>,
}

impl Shell {
    pub fn new(
        library: Result<Library, StoreError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = cx.new(|_| LibraryStore::new(library));
        Self {
            page: Page::Library,
            library_view: cx.new(|cx| LibraryView::new(store, window, cx)),
        }
    }

    fn nav_item(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        page: Page,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let active = self.page == page;
        v_flex()
            .id(id)
            .test_support()
            .w_full()
            .items_center()
            .gap_1()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .text_xs()
            .when(active, |this| {
                this.bg(theme.accent).text_color(theme.accent_foreground)
            })
            .when(!active, |this| {
                this.text_color(theme.muted_foreground)
                    .hover(|this| this.bg(theme.list_hover))
            })
            .child(Icon::new(icon).size(px(20.)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.page = page;
                cx.notify();
            }))
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rail = v_flex()
            .w(px(72.))
            .flex_shrink_0()
            .h_full()
            .gap_1()
            .p_2()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(self.nav_item("nav-library", IconName::LayoutDashboard, "Library", Page::Library, cx))
            .child(self.nav_item("nav-tools", IconName::Cpu, "Tools", Page::Tools, cx))
            .child(div().flex_1())
            .child(self.nav_item("nav-settings", IconName::Settings, "Settings", Page::Settings, cx));

        let page = match self.page {
            Page::Library => self.library_view.clone().into_any_element(),
            Page::Tools => placeholder("tools", "Tools", theme),
            Page::Settings => placeholder("settings", "Settings", theme),
        };

        h_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(rail)
            .child(div().flex_1().min_w_0().h_full().child(page))
    }
}

fn placeholder(id: &'static str, title: &'static str, theme: &Theme) -> AnyElement {
    v_flex()
        .id(id)
        .test_support()
        .size_full()
        .gap_4()
        .p_6()
        .child(div().text_lg().font_semibold().child(title))
        .child(
            div()
                .flex_1()
                .rounded_lg()
                .border_1()
                .border_dashed()
                .border_color(theme.border),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
