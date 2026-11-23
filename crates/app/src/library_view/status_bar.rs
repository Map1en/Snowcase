
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::i18n::{self, tr};
use crate::library_store::{LibraryStore, Stage};
use crate::library_view::*;
use crate::library_view::widgets::format_size;

pub(crate) const UNSUPPORTED_LISTED: usize = 20;

impl LibraryView {
    pub(crate) fn render_status(&self, store: &LibraryStore, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let progress = store.progress();
        let mut notes: Vec<SharedString> = Vec::new();
        if store.is_scanning() {
            notes.push(tr!(cx, "status.scanning"));
        } else if self.pending_imports > 0 || store.is_busy() {
            notes.push(tr!(cx, "status.adding"));
        } else if self.last_failures > 0 {
            notes.push(tr!(cx, "status.failures", count = self.last_failures));
        }
        let unsupported = (!store.unsupported().is_empty()).then(|| {
            let listed: Vec<&str> = store.unsupported().iter().take(UNSUPPORTED_LISTED).map(String::as_str).collect();
            Button::new("unsupported")
                .ghost()
                .xsmall()
                .label(tr!(cx, "status.unsupported", count = store.unsupported().len()))
                .tooltip(listed.join("\n"))
        });
        let background = progress.running.then(|| {
            let mut label = match progress.stage {
                Stage::Files => tr!(cx, "status.background", done = progress.done, total = progress.total).to_string(),
                Stage::Packages => {
                    let size = |bytes: u64| format_size(i64::try_from(bytes).unwrap_or(i64::MAX));
                    let mut label =
                        tr!(cx, "status.packages", done = size(progress.bytes_done), total = size(progress.bytes_total))
                            .to_string();
                    let speed = progress.since.filter(|_| !progress.paused).and_then(|(since, bytes)| {
                        let seconds = cx.background_executor().now().duration_since(since).as_secs_f64();
                        (seconds >= 1.).then(|| (progress.bytes_done.saturating_sub(bytes) as f64 / seconds) as u64)
                    });
                    if let Some(speed) = speed {
                        label.push_str(&format!(" · {}/s", size(speed)));
                    }
                    label
                }
            };
            if progress.paused {
                label.push_str(" · ");
                label.push_str(&tr!(cx, "status.paused"));
            }
            let paused = progress.paused;
            h_flex()
                .gap_2()
                .items_center()
                .child(div().id("background-progress").child(label))
                .child(
                    Button::new("pause-background")
                        .outline()
                        .xsmall()
                        .label(if paused { tr!(cx, "action.resume") } else { tr!(cx, "action.pause") })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |store, cx| {
                                if paused {
                                    store.resume_background(cx);
                                } else {
                                    store.pause_background(cx);
                                }
                            });
                        })),
                )
                .child(
                    Button::new("cancel-background")
                        .ghost()
                        .xsmall()
                        .label(tr!(cx, "action.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store.update(cx, |store, cx| store.cancel_background(cx));
                        })),
                )
        });
        h_flex()
            .id("status-bar")
            .test_support()
            .gap_3()
            .px_4()
            .py_1()
            .min_h(px(30.))
            .items_center()
            .border_t_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .children(background)
            .children(notes.into_iter().map(|note| div().child(note)))
            .children(unsupported)
            .child(div().flex_1())
            .children(store.error().map(|error| {
                div()
                    .text_color(theme.danger)
                    .truncate()
                    .child(i18n::error_message(error, cx))
            }))
    }
}
