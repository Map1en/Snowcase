use std::path::PathBuf;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::FileKind;
use snowcase_core::library::{Asset, AssetId, Library, Scope, SortKey, inspect_file};

const KINDS: [FileKind; 5] = [
    FileKind::UnityPackage,
    FileKind::Archive,
    FileKind::Image,
    FileKind::Model,
    FileKind::Other,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Grid,
    Table,
}

pub struct LibraryView {
    library: Library,
    scope: Scope,
    sort: SortKey,
    layout: Layout,
    selected: Option<AssetId>,
    search: Entity<InputState>,
    tag_input: Entity<InputState>,
    folder_input: Entity<InputState>,
    pending_adds: usize,
    last_failures: usize,
    _subscriptions: Vec<Subscription>,
}

impl LibraryView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search names and tags"));
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a tag, then Enter"));
        let folder_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Move to folder, then Enter"));
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |_, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&tag_input, window, |this, input, event, window, cx| {
                if let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.selected) {
                    let tag = input.read(cx).value();
                    this.library.add_tag(id, &tag);
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    cx.notify();
                }
            }),
            cx.subscribe_in(&folder_input, window, |this, input, event, window, cx| {
                if let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.selected) {
                    let folder = input.read(cx).value();
                    if !folder.trim().is_empty() {
                        this.library.move_to_folder(id, Some(&folder));
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                        cx.notify();
                    }
                }
            }),
        ];
        Self {
            library: Library::default(),
            scope: Scope::All,
            sort: SortKey::Added,
            layout: Layout::Grid,
            selected: None,
            search,
            tag_input,
            folder_input,
            pending_adds: 0,
            last_failures: 0,
            _subscriptions: subscriptions,
        }
    }

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

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        cx.notify();
    }

    fn render_filters(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let muted = cx.theme().muted_foreground;
        let mut column = v_flex()
            .id("filters")
            .w(px(220.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .gap_4()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        let library = [
            ("scope-all", "All assets", Scope::All),
            ("scope-untagged", "Untagged", Scope::Untagged),
            ("scope-unfiled", "Not in a folder", Scope::Unfiled),
        ];
        column = column.child(filter_group("Library").children(library.into_iter().map(
            |(id, label, scope)| {
                let count = self.library.count(&scope);
                self.filter_item(SharedString::from(id), label.into(), count, scope, cx)
            },
        )));

        column = column.child(filter_group("Types").children(KINDS.into_iter().map(|kind| {
            let scope = Scope::Kind(kind);
            let count = self.library.count(&scope);
            self.filter_item(
                format!("scope-kind-{kind:?}").into(),
                kind_label(kind).into(),
                count,
                scope,
                cx,
            )
        })));

        let folders: Vec<_> = self.library.folders().map(str::to_owned).collect();
        let mut folder_group = filter_group("Folders");
        if folders.is_empty() {
            folder_group = folder_group.child(hint("Move an asset to a folder from its details", muted));
        }
        column = column.child(folder_group.children(folders.into_iter().map(|folder| {
            let scope = Scope::Folder(folder.clone());
            let count = self.library.count(&scope);
            self.filter_item(
                format!("scope-folder-{folder}").into(),
                folder.into(),
                count,
                scope,
                cx,
            )
        })));

        let tags: Vec<_> = self
            .library
            .tags()
            .into_iter()
            .map(|(tag, count)| (tag.to_owned(), count))
            .collect();
        let mut tag_group = filter_group("Tags");
        if tags.is_empty() {
            tag_group = tag_group.child(hint("Tag assets from their details", muted));
        }
        column.child(tag_group.children(tags.into_iter().map(|(tag, count)| {
            self.filter_item(
                format!("scope-tag-{tag}").into(),
                format!("# {tag}").into(),
                count,
                Scope::Tag(tag),
                cx,
            )
        })))
    }

    fn filter_item(
        &self,
        id: SharedString,
        label: SharedString,
        count: usize,
        scope: Scope,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.scope == scope;
        let theme = cx.theme();
        h_flex()
            .id(ElementId::Name(id))
            .test_support()
            .justify_between()
            .px_2()
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

    fn render_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut status = format!("{} assets", self.library.assets().len());
        if self.pending_adds > 0 {
            status.push_str(" · adding…");
        } else if self.last_failures > 0 {
            status.push_str(&format!(" · {} could not be added", self.last_failures));
        }

        let sort = self.sort;
        let layout = self.layout;
        h_flex()
            .gap_2()
            .items_center()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .w(px(260.))
                    .child(Input::new(&self.search).id("search").cleanable(true)),
            )
            .child(
                ButtonGroup::new("sort")
                    .outline()
                    .compact()
                    .children([
                        sort_button("sort-added", "Newest", SortKey::Added, sort),
                        sort_button("sort-name", "Name", SortKey::Name, sort),
                        sort_button("sort-size", "Size", SortKey::Size, sort),
                    ])
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        let sorts = [SortKey::Added, SortKey::Name, SortKey::Size];
                        if let Some(sort) = selected.first().and_then(|ix| sorts.get(*ix)) {
                            this.sort = *sort;
                            cx.notify();
                        }
                    })),
            )
            .child(
                ButtonGroup::new("layout")
                    .outline()
                    .compact()
                    .children([
                        Button::new("layout-grid")
                            .label("Grid")
                            .selected(layout == Layout::Grid),
                        Button::new("layout-table")
                            .label("Table")
                            .selected(layout == Layout::Table),
                    ])
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        this.layout = match selected.first() {
                            Some(1) => Layout::Table,
                            _ => Layout::Grid,
                        };
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(div().text_sm().text_color(muted).child(status))
            .child(
                Button::new("add-files")
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add files")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
            )
    }

    fn render_grid(&self, assets: &[&Asset], cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .id("asset-grid")
            .flex_1()
            .overflow_y_scroll()
            .p_4()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_3()
                    .children(assets.iter().enumerate().map(|(ix, asset)| {
                        let id = asset.id;
                        let selected = self.selected == Some(id);
                        v_flex()
                            .id(("asset", ix))
                            .test_support()
                            .w(px(160.))
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .border_2()
                            .cursor_pointer()
                            .border_color(if selected {
                                theme.ring
                            } else {
                                gpui_kit::transparent_black()
                            })
                            .hover(|this| this.bg(theme.list_hover))
                            .child(thumbnail(asset.file.kind, px(112.), &theme))
                            .child(div().text_sm().truncate().child(asset.file.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format_size(asset.file.size)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
                    })),
            )
            .into_any_element()
    }

    fn render_table(&self, assets: &[&Asset], cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let header = table_row()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.list_head)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(name_cell().child("Name"))
            .child(kind_cell().child("Type"))
            .child(tags_cell().child("Tags"))
            .child(folder_cell().child("Folder"))
            .child(size_cell().child("Size"));
        v_flex()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id("asset-table")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(assets.iter().enumerate().map(|(ix, asset)| {
                        let id = asset.id;
                        let selected = self.selected == Some(id);
                        let tags = asset.tags.iter().cloned().collect::<Vec<_>>().join(", ");
                        table_row()
                            .id(("asset", ix))
                            .test_support()
                            .py_1p5()
                            .text_sm()
                            .cursor_pointer()
                            .when(ix % 2 == 1, |this| this.bg(theme.list_even))
                            .when(selected, |this| this.bg(theme.list_active))
                            .hover(|this| this.bg(theme.list_hover))
                            .child(name_cell().truncate().child(asset.file.name.clone()))
                            .child(
                                kind_cell()
                                    .text_color(theme.muted_foreground)
                                    .child(kind_label(asset.file.kind)),
                            )
                            .child(tags_cell().truncate().text_color(theme.muted_foreground).child(tags))
                            .child(
                                folder_cell()
                                    .truncate()
                                    .text_color(theme.muted_foreground)
                                    .child(asset.folder.clone().unwrap_or_default()),
                            )
                            .child(
                                size_cell()
                                    .text_color(theme.muted_foreground)
                                    .child(format_size(asset.file.size)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
                    })),
            )
            .into_any_element()
    }

    fn select(&mut self, id: AssetId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn render_detail(&self, asset: &Asset, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let id = asset.id;
        let path = asset.file.path.clone();
        let tags: Vec<_> = asset.tags.iter().cloned().collect();
        v_flex()
            .id("detail")
            .test_support()
            .w(px(320.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .gap_4()
            .p_4()
            .border_l_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .font_semibold()
                            .min_w_0()
                            .child(asset.file.name.clone()),
                    )
                    .child(
                        Button::new("close-detail")
                            .ghost()
                            .compact()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selected = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(thumbnail(asset.file.kind, px(180.), &theme))
            .child(
                v_flex()
                    .gap_1()
                    .child(field("Type", kind_label(asset.file.kind).into(), &theme))
                    .child(field("Size", format_size(asset.file.size).into(), &theme))
                    .child(field("Location", path.display().to_string().into(), &theme)),
            )
            .child(
                Button::new("reveal")
                    .outline()
                    .compact()
                    .icon(IconName::FolderOpen)
                    .label("Show in Explorer")
                    .on_click(move |_, _, cx| cx.reveal_path(&path)),
            )
            .child(
                section("Folder")
                    .child(
                        h_flex()
                            .justify_between()
                            .text_sm()
                            .child(asset.folder.clone().unwrap_or_else(|| "None".into()))
                            .when(asset.folder.is_some(), |this| {
                                this.child(
                                    Button::new("remove-folder")
                                        .ghost()
                                        .compact()
                                        .label("Remove")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.library.move_to_folder(id, None);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(Input::new(&self.folder_input).id("folder-input")),
            )
            .child(
                section("Tags")
                    .child(h_flex().flex_wrap().gap_1().children(tags.into_iter().enumerate().map(
                        |(ix, tag)| {
                            Button::new(("remove-tag", ix))
                                .outline()
                                .compact()
                                .label(format!("{tag}  ×"))
                                .tooltip("Remove tag")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library.remove_tag(id, &tag);
                                    cx.notify();
                                }))
                        },
                    )))
                    .child(Input::new(&self.tag_input).id("tag-input")),
            )
            .child(planned("Source", "Store page, author and purchase info", &theme))
            .child(planned("Contents", "Files inside the package", &theme))
            .child(planned("License", "VN3 terms and restrictions", &theme))
            .child(planned("Used in", "Unity projects and uploaded avatars", &theme))
    }
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let search = self.search.read(cx).value();
        let assets = self.library.query(&self.scope, &search, self.sort);

        let body = if self.library.assets().is_empty() {
            centered("empty-state", "Drop files here, or click Add files", cx)
        } else if assets.is_empty() {
            centered("no-results", "Nothing matches this filter", cx)
        } else {
            match self.layout {
                Layout::Grid => self.render_grid(&assets, cx),
                Layout::Table => self.render_table(&assets, cx),
            }
        };
        let detail = self
            .selected
            .and_then(|id| self.library.get(id))
            .map(|asset| self.render_detail(asset, cx));
        let filters = self.render_filters(cx);

        h_flex()
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_paths(paths.paths().to_vec(), cx);
            }))
            .child(filters)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_toolbar(cx))
                    .child(body),
            )
            .children(detail)
    }
}

fn sort_button(id: &'static str, label: &'static str, key: SortKey, current: SortKey) -> Button {
    Button::new(id).label(label).selected(key == current)
}

fn filter_group(label: &'static str) -> Div {
    v_flex().gap_0p5().child(
        div()
            .px_2()
            .pb_1()
            .text_xs()
            .font_semibold()
            .opacity(0.6)
            .child(label),
    )
}

fn hint(text: &'static str, color: Hsla) -> Div {
    div().px_2().text_xs().text_color(color).child(text)
}

fn section(label: &'static str) -> Div {
    v_flex()
        .gap_2()
        .child(div().text_xs().font_semibold().opacity(0.6).child(label))
}

fn field(label: &'static str, value: SharedString, theme: &Theme) -> Div {
    h_flex()
        .gap_2()
        .text_sm()
        .items_start()
        .child(
            div()
                .w(px(72.))
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(value))
}

fn planned(label: &'static str, text: &'static str, theme: &Theme) -> Div {
    section(label).child(
        div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(format!("{text} · planned")),
    )
}

fn centered(id: &'static str, text: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn thumbnail(kind: FileKind, height: Pixels, theme: &Theme) -> Div {
    let icon = match kind {
        FileKind::UnityPackage => IconName::Frame,
        FileKind::Archive => IconName::FolderClosed,
        FileKind::Image => IconName::GalleryVerticalEnd,
        FileKind::Model => IconName::Frame,
        FileKind::Other => IconName::File,
    };
    v_flex()
        .w_full()
        .h(height)
        .rounded_md()
        .bg(theme.muted)
        .items_center()
        .justify_center()
        .gap_1()
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size(px(28.)))
        .child(div().text_xs().child(kind_label(kind)))
}

fn table_row() -> Div {
    h_flex().gap_3().px_4()
}

fn name_cell() -> Div {
    div().flex_1().min_w(px(160.))
}

fn kind_cell() -> Div {
    div().w(px(110.)).flex_shrink_0()
}

fn tags_cell() -> Div {
    div().w(px(160.)).flex_shrink_0()
}

fn folder_cell() -> Div {
    div().w(px(120.)).flex_shrink_0()
}

fn size_cell() -> Div {
    div().w(px(80.)).flex_shrink_0().text_right()
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
