use std::path::Path;

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::snapshot::{Hit, PackageSummary, Snapshot, shown_name};
use snowcase_core::{BlobId, ItemId, LocationState};

use crate::i18n::tr;
use crate::library_view::*;
use crate::library_view::widgets::*;
use super::package_tree;

pub(crate) const TREE_FILES: usize = 200;

pub(crate) type TreeNode = (BlobId, String, String);

pub(crate) struct Tree<'a> {
    blob: BlobId,
    member: String,
    paths: &'a [String],
    hit: Option<&'a str>,
}

impl LibraryView {
    pub(crate) fn render_files(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let hit = library.hit(id, &self.filter.search);
        let hit_path = match &hit {
            Some(Hit::Package { path, .. }) => Some(path.clone()),
            _ => None,
        };
        let hit_file = match hit {
            Some(Hit::File(name)) => Some(name),
            Some(Hit::Package { file, .. }) => Some(file),
            Some(Hit::Tag { file, .. }) => file,
            _ => None,
        };
        let mut tree_rows = (0usize, 0usize);
        let mut list = section(tr!(cx, "item.files"));
        let mut package_rows = 0usize;
        for (ix, file) in library.files(id).into_iter().enumerate() {
            let name = shown_name(file.file).to_owned();
            let present = file
                .locations
                .iter()
                .find(|location| location.state == LocationState::Present)
                .map(|location| library.location_path(location));
            let tags: Vec<String> = library
                .file_tags(id, file.blob.id)
                .into_iter()
                .map(|tag| format!("#{}", tag.name))
                .collect();
            let highlighted = hit_file.as_deref().is_some_and(|hit| hit == name || hit == file.file.file_name);
            list = list.child(
                h_flex()
                    .id(("file", ix))
                    .test_support()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .rounded_md()
                    .text_sm()
                    .when(highlighted, |this| this.bg(theme.warning.opacity(0.2)))
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .when(file.file.extracted_from_blob_id.is_some(), |this| {
                        this.child(badge(tr!(cx, "file.extracted"), theme))
                    })
                    .when(present.is_none(), |this| this.child(badge(tr!(cx, "file.missing"), theme)))
                    .children(tags.into_iter().map(|tag| badge(tag, theme)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format_size(file.blob.size)),
                    )
                    .children(present.map(|path| {
                        Button::new(("reveal", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::FolderOpen)
                            .tooltip(tr!(cx, "action.reveal"))
                            .on_click(move |_, _, cx| cx.reveal_path(Path::new(&path)))
                    })),
            );
            let packages = library
                .package_of(file.file)
                .map(|summary| (None, summary))
                .into_iter()
                .chain(library.packages_in(file.blob.id).into_iter().map(|(member, summary)| (Some(member), summary)));
            for (member, summary) in packages {
                let package = (file.blob.id, member.unwrap_or_default().to_owned());
                let node: TreeNode = (package.0, package.1.clone(), String::new());
                let hit = hit_path.as_deref().filter(|path| summary.paths.iter().any(|own| own == path));
                let open = self.open_nodes.contains(&node) || hit.is_some();
                let text = format!("{} {}", if open { "▾" } else { "▸" }, package_label(member, summary, cx));
                list = list.child(
                    div()
                        .id(("package", package_rows))
                        .test_support()
                        .pl_4()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .cursor_pointer()
                        .hover(|this| this.text_color(theme.foreground))
                        .child(text)
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_node(node.clone(), cx))),
                );
                package_rows += 1;
                if open {
                    let tree = Tree {
                        blob: package.0,
                        member: package.1.clone(),
                        paths: &summary.paths,
                        hit,
                    };
                    let top = package_tree::top(&summary.paths);
                    list = list.children(self.tree_rows(&tree, &top, 2, &mut tree_rows, cx));
                }
            }
        }
        let hints = library.hints(id);
        if hints.extracted > 0 {
            list = list.child(hint(tr!(cx, "hint.extracted", count = hints.extracted), theme));
        }
        if hints.duplicates > 0 {
            list = list.child(hint(tr!(cx, "hint.duplicates", count = hints.duplicates), theme));
        }
        for (ix, (other, version)) in hints.versions.into_iter().enumerate() {
            list = list.child(
                Button::new(("other-version", ix))
                    .ghost()
                    .small()
                    .label(tr!(cx, "hint.version", version = version))
                    .on_click(cx.listener(move |this, _, window, cx| this.select(Some(other), window, cx))),
            );
        }
        for (ix, other) in library.merge_candidates(id).into_iter().enumerate() {
            let title = library.item(other).map(|item| item.title.clone()).unwrap_or_default();
            list = list.child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(hint(tr!(cx, "hint.same_package", title = title), theme).flex_1().min_w_0())
                    .child(
                        Button::new(("merge", ix))
                            .outline()
                            .xsmall()
                            .label(tr!(cx, "action.merge_here"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change(cx, move |library| library.merge_items(id, other));
                            })),
                    ),
            );
        }
        list
    }

    pub(crate) fn toggle_node(&mut self, node: TreeNode, cx: &mut Context<Self>) {
        if !self.open_nodes.remove(&node) {
            self.open_nodes.insert(node);
        }
        cx.notify();
    }

    pub(crate) fn tree_rows(&self, tree: &Tree, folder: &str, depth: usize, rows: &mut (usize, usize), cx: &Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let indent = px(12. * depth as f32);
        let entries = package_tree::entries(tree.paths, folder);
        let mut out = Vec::new();
        for child in entries.folders {
            let node: TreeNode = (tree.blob, tree.member.clone(), child.clone());
            let open = self.open_nodes.contains(&node) || tree.hit.is_some_and(|hit| package_tree::holds(&child, hit));
            let name = format!("{} {}", if open { "▾" } else { "▸" }, package_tree::shown(&child, folder));
            out.push(
                div()
                    .id(("tree-folder", rows.0))
                    .test_support()
                    .pl(indent)
                    .text_xs()
                    .truncate()
                    .cursor_pointer()
                    .hover(|this| this.bg(theme.list_hover))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_node(node.clone(), cx)))
                    .into_any_element(),
            );
            rows.0 += 1;
            if open {
                out.extend(self.tree_rows(tree, &child, depth + 1, rows, cx));
            }
        }
        for (n, file) in entries.files.iter().enumerate() {
            if n == TREE_FILES {
                let more = tr!(cx, "tree.more", count = entries.files.len() - TREE_FILES);
                out.push(div().pl(indent).text_xs().text_color(theme.muted_foreground).child(more).into_any_element());
                break;
            }
            let highlighted = tree.hit == Some(*file);
            out.push(
                div()
                    .id(("tree-file", rows.1))
                    .test_support()
                    .pl(indent)
                    .text_xs()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .when(highlighted, |this| this.bg(theme.warning.opacity(0.2)).text_color(theme.foreground))
                    .child(package_tree::shown(file, folder).to_owned())
                    .into_any_element(),
            );
            rows.1 += 1;
        }
        out
    }
}

pub(crate) fn package_label(member: Option<&str>, summary: &PackageSummary, cx: &App) -> String {
    let mut parts: Vec<String> = member
        .map(|member| member.rsplit('/').next().unwrap_or(member).to_owned())
        .into_iter()
        .collect();
    match (&summary.error, &summary.root) {
        (Some(_), _) => parts.push(tr!(cx, "package.failed").to_string()),
        (None, root) => {
            parts.push(tr!(cx, "package.files", count = summary.files).to_string());
            parts.extend(root.clone());
        }
    }
    parts.join(" · ")
}
