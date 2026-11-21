use std::path::Path;

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Scope, Snapshot};
use snowcase_core::RootId;

use crate::i18n::{self, tr};
use crate::library_view::*;

pub(crate) const NAV_WIDTH: f32 = 220.;

impl LibraryView {
    pub(crate) fn render_nav(&self, library: &Snapshot, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let mut column = v_flex()
            .id("filters")
            .test_support()
            .w(px(NAV_WIDTH))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .gap_4()
            .p_3()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar);

        column = column.child(filter_group(tr!(cx, "nav.library"), []).child(self.nav_item(
            "scope-all".into(),
            tr!(cx, "scope.all"),
            0,
            library.count(&Scope::All),
            Scope::All,
            cx,
        )));

        let add = Button::new("add-folder")
            .ghost()
            .compact()
            .icon(IconName::Plus)
            .tooltip(tr!(cx, "folder.new"))
            .on_click(cx.listener(|this, _, _, cx| this.add_folder(cx)));
        let mut folders = filter_group(tr!(cx, "item.folders"), [add]);
        for row in library.folder_tree() {
            folders = folders.child(self.nav_item(
                format!("scope-folder-{}", row.folder.id.0).into(),
                row.folder.name.clone().into(),
                row.depth,
                row.count,
                Scope::Folder(row.folder.id),
                cx,
            ));
        }
        column = column.child(folders);

        let rescan = Button::new("rescan")
            .ghost()
            .compact()
            .icon(IconName::RefreshCw)
            .loading(self.store.read(cx).is_scanning())
            .tooltip(tr!(cx, "root.rescan"))
            .on_click(cx.listener(|this, _, _, cx| this.scan(None, cx)));
        let add = Button::new("add-root")
            .ghost()
            .compact()
            .icon(IconName::Plus)
            .tooltip(tr!(cx, "root.add"))
            .on_click(cx.listener(|this, _, _, cx| this.pick_root(cx)));
        let actions = if library.roots().is_empty() { vec![add] } else { vec![rescan, add] };
        let mut roots = filter_group(tr!(cx, "root.title"), actions);
        for (root, count) in library.roots() {
            let root_id = root.id;
            let doomed = library.only_in_root(root_id);
            let path = root.path.clone();
            roots = roots.child(
                h_flex()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(self.nav_item(
                        format!("scope-root-{}", root.id.0).into(),
                        folder_name(&root.path).into(),
                        0,
                        count,
                        Scope::Root(root.id),
                        cx,
                    )))
                    .child(
                        Button::new(("remove-root", root.id.0 as usize))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip(tr!(cx, "root.remove"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_remove_root(root_id, &path, doomed, window, cx);
                            })),
                    ),
            );
        }
        column.child(roots)
    }

    pub(crate) fn confirm_remove_root(&mut self, root: RootId, path: &str, doomed: usize, window: &mut Window, cx: &mut Context<Self>) {
        let message = tr!(cx, "root.remove_confirm", path = path);
        let detail = tr!(cx, "root.remove_detail", count = doomed);
        let (remove, cancel) = (tr!(cx, "root.remove"), tr!(cx, "action.cancel"));
        let answer = window.prompt(PromptLevel::Warning, &message, Some(&detail), &[remove.as_ref(), cancel.as_ref()], cx);
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |this, cx| {
                    if this.filter.scope == Scope::Root(root) {
                        this.filter.scope = Scope::All;
                    }
                    this.change(cx, move |library| library.remove_root(root));
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn nav_item(
        &self,
        id: SharedString,
        label: SharedString,
        depth: usize,
        count: usize,
        scope: Scope,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let active = self.filter.scope == scope;
        h_flex()
            .id(ElementId::Name(id))
            .test_support()
            .justify_between()
            .pl(px(8. + 12. * depth as f32))
            .pr_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .when(active, |this| {
                this.bg(theme.accent).text_color(theme.accent_foreground)
            })
            .when(!active, |this| this.hover(|this| this.bg(theme.list_hover)))
            .child(div().truncate().child(label))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_scope(scope.clone(), cx)))
    }

    pub(crate) fn pick_root(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                this.update(cx, |this, cx| {
                    for path in paths {
                        this.scan(Some(path), cx);
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn add_folder(&mut self, cx: &mut Context<Self>) {
        let locale = i18n::locale(cx);
        self.change(cx, move |library| {
            let taken: Vec<String> = library
                .folder_tree()
                .iter()
                .map(|row| row.folder.name.to_lowercase())
                .collect();
            let name = (1..)
                .map(|n| rust_i18n::t!("folder.new_name", locale = locale, n = n).into_owned())
                .find(|name| !taken.contains(&name.to_lowercase()))
                .unwrap_or_default();
            library.create_folder(None, &name)
        });
    }
}

pub(crate) fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |name| name.to_string_lossy().into_owned())
}

pub(crate) fn filter_group(label: SharedString, actions: impl IntoIterator<Item = Button>) -> Div {
    v_flex().gap_0p5().child(
        h_flex()
            .justify_between()
            .items_center()
            .px_2()
            .pb_1()
            .child(div().flex_1().text_xs().font_semibold().opacity(0.6).child(label))
            .children(actions),
    )
}
