use std::path::PathBuf;

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use snowcase_core::FileKind;
use snowcase_core::library::{Asset, Library, inspect_file};

#[derive(Default)]
pub struct LibraryView {
    library: Library,
    pending_adds: usize,
    last_failures: usize,
}

impl LibraryView {
    fn pick_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                this.update(cx, |this, cx| this.add_paths(paths, cx)).ok();
            }
        })
        .detach();
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.pending_adds += 1;
        cx.notify();
        let inspected = cx.background_spawn(async move {
            paths.iter().map(|path| inspect_file(path)).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = inspected.await;
            this.update(cx, |this, cx| {
                let (files, failures): (Vec<_>, Vec<_>) =
                    results.into_iter().partition(Result::is_ok);
                this.library.insert(files.into_iter().flatten());
                this.last_failures = failures.len();
                this.pending_adds -= 1;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let assets = self.library.assets();

        let mut status = format!("{} files", assets.len());
        if self.pending_adds > 0 {
            status.push_str(" · adding…");
        } else if self.last_failures > 0 {
            status.push_str(&format!(" · {} could not be added", self.last_failures));
        }

        let header = h_flex()
            .justify_between()
            .items_center()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .gap_3()
                    .items_baseline()
                    .child(div().text_lg().font_semibold().child("Library"))
                    .child(div().text_sm().text_color(muted).child(status)),
            )
            .child(
                Button::new("add-files")
                    .primary()
                    .label("Add files")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
            );

        let body = if assets.is_empty() {
            div()
                .id("empty-state")
                .test_support()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("Drop files here, or click Add files")
                .into_any_element()
        } else {
            div()
                .id("asset-list")
                .flex_1()
                .overflow_y_scroll()
                .children(
                    assets
                        .iter()
                        .enumerate()
                        .map(|(ix, asset)| asset_row(ix, asset, muted)),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_paths(paths.paths().to_vec(), cx);
            }))
            .child(header)
            .child(body)
    }
}

fn asset_row(ix: usize, asset: &Asset, muted: Hsla) -> impl IntoElement {
    h_flex()
        .id(("asset", ix))
        .test_support()
        .gap_4()
        .px_4()
        .py_2()
        .child(
            div()
                .w(px(96.))
                .text_sm()
                .text_color(muted)
                .child(kind_label(asset.file.kind)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().truncate().child(asset.file.name.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(asset.file.path.display().to_string()),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(muted)
                .child(format_size(asset.file.size)),
        )
}

fn kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::UnityPackage => "Unity package",
        FileKind::Archive => "Archive",
        FileKind::Image => "Image",
        FileKind::Model => "3D model",
        FileKind::Other => "File",
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("B"))
    }
}

#[cfg(test)]
mod tests;
