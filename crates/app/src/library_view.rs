mod item_table;

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::library::{Library, Scope, SortKey, collect_files, inspect_file};
use snowcase_core::{FileKind, ItemChanges, ItemId, ItemStatus, LocationState, StoreError};

use self::item_table::{ItemRow, ItemTable};
use crate::library_store::LibraryStore;

const CARD_WIDTH: f32 = 168.;
const CARD_HEIGHT: f32 = 196.;
const GRID_GAP: f32 = 12.;
const GRID_PADDING: f32 = 16.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Grid,
    Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Overview,
    Files,
}

const DETAIL_TABS: [(DetailTab, &str); 2] =
    [(DetailTab::Overview, "Overview"), (DetailTab::Files, "Files")];

pub struct LibraryView {
    store: Entity<LibraryStore>,
    scope: Scope,
    sort: SortKey,
    layout: Layout,
    rows: Vec<ItemId>,
    selected: Option<ItemId>,
    page: Option<(ItemId, DetailTab)>,
    grid_width: Pixels,
    search: Entity<InputState>,
    tag_input: Entity<InputState>,
    folder_input: Entity<InputState>,
    table: Entity<TableState<ItemTable>>,
    pending_imports: usize,
    last_failures: usize,
    _subscriptions: Vec<Subscription>,
}

impl LibraryView {
    pub fn new(store: Entity<LibraryStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let search = input("Search titles, tags and files", window, cx);
        let tag_input = input("Add a tag", window, cx);
        let folder_input = input("Add to a folder", window, cx);
        let table = cx.new(|cx| TableState::new(ItemTable::default(), window, cx));
        let subscriptions = vec![
            cx.observe(&store, |this, _, cx| this.refresh(cx)),
            cx.subscribe_in(&search, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh(cx);
                }
            }),
            cx.subscribe_in(&tag_input, window, |this, input, event, window, cx| {
                let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.current_item())
                else {
                    return;
                };
                let name = input.read(cx).value();
                this.change(cx, |library| library.tag_item(id, &name));
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }),
            cx.subscribe_in(&folder_input, window, |this, input, event, window, cx| {
                let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.current_item())
                else {
                    return;
                };
                let name = input.read(cx).value();
                this.change(cx, |library| {
                    if let Some(folder) = library.create_folder(None, &name)? {
                        library.put_in_folder(id, folder)?;
                    }
                    Ok(())
                });
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }),
            cx.subscribe_in(&table, window, |this, table, event, _, cx| {
                let row = |ix: &usize| table.read(cx).delegate().rows.get(*ix).map(|row| row.id);
                match event {
                    TableEvent::SelectRow(ix) => {
                        if let Some(id) = row(ix) {
                            this.select(id, cx);
                        }
                    }
                    TableEvent::DoubleClickedRow(ix) => {
                        if let Some(id) = row(ix) {
                            this.open_page(id, cx);
                        }
                    }
                    _ => {}
                }
            }),
        ];
        let mut view = Self {
            store,
            scope: Scope::All,
            sort: SortKey::Added,
            layout: Layout::Grid,
            rows: Vec::new(),
            selected: None,
            page: None,
            grid_width: px(0.),
            search,
            tag_input,
            folder_input,
            table,
            pending_imports: 0,
            last_failures: 0,
            _subscriptions: subscriptions,
        };
        view.refresh(cx);
        view
    }

    fn current_item(&self) -> Option<ItemId> {
        self.page.map(|(id, _)| id).or(self.selected)
    }

    fn change<T>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Library) -> Result<T, StoreError>,
    ) -> Option<T> {
        self.store.update(cx, |store, cx| store.change(cx, change))
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let search = self.search.read(cx).value();
        let Some(library) = self.store.read(cx).library() else {
            return;
        };
        self.rows = library
            .query(&self.scope, &search, self.sort)
            .into_iter()
            .map(|item| item.id)
            .collect();
        let exists = |id: &ItemId| library.item(*id).is_some();
        self.selected = self.selected.filter(exists);
        self.page = self.page.filter(|(id, _)| exists(id));
        let rows: Vec<ItemRow> = self
            .rows
            .iter()
            .filter_map(|id| ItemRow::new(library, *id))
            .collect();
        self.table.update(cx, |table, cx| {
            table.delegate_mut().rows = rows;
            table.refresh(cx);
            cx.notify();
        });
        cx.notify();
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
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
        self.pending_imports += 1;
        cx.notify();
        let inspected = cx.background_spawn(async move {
            collect_files(&paths)
                .iter()
                .map(|path| inspect_file(path))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = inspected.await;
            this.update(cx, |this, cx| {
                let (files, failures): (Vec<_>, Vec<_>) =
                    results.into_iter().partition(Result::is_ok);
                this.last_failures = failures.len();
                this.pending_imports -= 1;
                let files = files.into_iter().flatten().collect();
                this.change(cx, |library| library.import(files));
            })
            .ok();
        })
        .detach();
    }

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.refresh(cx);
    }

    fn select(&mut self, id: ItemId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn open_page(&mut self, id: ItemId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.page = Some((id, DetailTab::Overview));
        cx.notify();
    }

    fn add_folder(&mut self, cx: &mut Context<Self>) {
        self.change(cx, |library| {
            let taken: Vec<String> = library
                .folder_tree()
                .iter()
                .map(|row| row.folder.name.to_lowercase())
                .collect();
            let name = (1..)
                .map(|n| format!("New folder {n}"))
                .find(|name| !taken.contains(&name.to_lowercase()))
                .unwrap_or_default();
            library.create_folder(None, &name)
        });
    }

    fn render_filters(&self, library: &Library, cx: &Context<Self>) -> Stateful<Div> {
        let theme = cx.theme();
        let mut column = v_flex()
            .id("filters")
            .size_full()
            .overflow_y_scroll()
            .gap_4()
            .p_3()
            .bg(theme.sidebar);

        let mut group = filter_group("Library", None);
        for (id, label, scope) in [
            ("scope-all", "All items", Scope::All),
            ("scope-inbox", "Inbox", Scope::Inbox),
            ("scope-in-library", "In library", Scope::InLibrary),
        ] {
            let count = library.count(&scope);
            group = group.child(self.filter_item(id.into(), label.into(), 0, count, scope, cx));
        }
        column = column.child(group);

        let add = Button::new("add-folder")
            .ghost()
            .compact()
            .icon(IconName::Plus)
            .tooltip("New folder")
            .on_click(cx.listener(|this, _, _, cx| this.add_folder(cx)));
        let mut group = filter_group("Folders", Some(add));
        for row in library.folder_tree() {
            group = group.child(self.filter_item(
                format!("scope-folder-{}", row.folder.id.0).into(),
                row.folder.name.clone().into(),
                row.depth,
                row.count,
                Scope::Folder(row.folder.id),
                cx,
            ));
        }
        column = column.child(group);

        let tags = library.tag_counts();
        if tags.is_empty() {
            return column;
        }
        let mut group = filter_group("Tags", None);
        for (tag, count) in tags {
            group = group.child(self.filter_item(
                format!("scope-tag-{}", tag.id.0).into(),
                format!("# {}", tag.name).into(),
                0,
                count,
                Scope::Tag(tag.id),
                cx,
            ));
        }
        column.child(group)
    }

    fn filter_item(
        &self,
        id: SharedString,
        label: SharedString,
        depth: usize,
        count: usize,
        scope: Scope,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let active = self.scope == scope;
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

    fn render_toolbar(&self, cx: &Context<Self>) -> Div {
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
                    .flex_1()
                    .max_w(px(320.))
                    .child(Input::new(&self.search).id("search").cleanable(true)),
            )
            .child(
                ButtonGroup::new("sort")
                    .outline()
                    .compact()
                    .children([
                        Button::new("sort-added")
                            .label("Newest")
                            .selected(sort == SortKey::Added),
                        Button::new("sort-name")
                            .label("Name")
                            .selected(sort == SortKey::Name),
                        Button::new("sort-size")
                            .label("Size")
                            .selected(sort == SortKey::Size),
                    ])
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        this.sort = match selected.first() {
                            Some(1) => SortKey::Name,
                            Some(2) => SortKey::Size,
                            _ => SortKey::Added,
                        };
                        this.refresh(cx);
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
            .child(
                Button::new("add-files")
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add files")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
            )
    }

    fn render_status(&self, store: &LibraryStore, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let total = store.library().map_or(0, |library| library.items().len());
        let mut status = format!("{} of {total} items", self.rows.len());
        if self.pending_imports > 0 {
            status.push_str(" · adding…");
        } else if self.last_failures > 0 {
            status.push_str(&format!(" · {} files could not be added", self.last_failures));
        }
        h_flex()
            .gap_3()
            .px_4()
            .py_1()
            .border_t_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(status)
            .children(
                store
                    .error()
                    .map(|error| div().text_color(theme.danger).truncate().child(error.to_owned())),
            )
    }

    fn render_grid(&self, cx: &Context<Self>) -> AnyElement {
        let usable = f32::from(self.grid_width) - GRID_PADDING * 2. + GRID_GAP;
        let columns = ((usable / (CARD_WIDTH + GRID_GAP)).floor() as usize).max(1);
        let sizes = Rc::new(vec![
            size(self.grid_width, px(CARD_HEIGHT + GRID_GAP));
            self.rows.len().div_ceil(columns)
        ]);
        let view = cx.entity().downgrade();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |this, cx| {
                            if this.grid_width != bounds.size.width {
                                this.grid_width = bounds.size.width;
                                cx.notify();
                            }
                        })
                        .ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                v_virtual_list(cx.entity(), "asset-grid", sizes, move |this, range, _, cx| {
                    let store = this.store.read(cx);
                    let Some(library) = store.library() else {
                        return Vec::new();
                    };
                    range
                        .map(|row| {
                            let start = row * columns;
                            let end = (start + columns).min(this.rows.len());
                            h_flex()
                                .gap(px(GRID_GAP))
                                .px(px(GRID_PADDING))
                                .pt(px(GRID_GAP))
                                .children((start..end).map(|ix| this.render_card(ix, library, cx)))
                        })
                        .collect()
                })
                .size_full(),
            )
            .into_any_element()
    }

    fn render_card(&self, ix: usize, library: &Library, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(item) = self.rows.get(ix).and_then(|id| library.item(*id)) else {
            return div().into_any_element();
        };
        let id = item.id;
        let facts = library.facts(id).cloned().unwrap_or_default();
        v_flex()
            .id(("asset", ix))
            .test_support()
            .w(px(CARD_WIDTH))
            .h(px(CARD_HEIGHT))
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_2()
            .cursor_pointer()
            .border_color(if self.selected == Some(id) {
                theme.ring
            } else {
                transparent_black()
            })
            .hover(|this| this.bg(theme.list_hover))
            .child(thumbnail(facts.kinds.first().copied(), px(124.), theme))
            .child(div().text_sm().truncate().child(item.title.clone()))
            .child(
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format_size(facts.size))
                    .when(item.status == ItemStatus::Inbox, |this| {
                        this.child(badge("Inbox", theme))
                    }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    this.open_page(id, cx);
                } else {
                    this.select(id, cx);
                }
            }))
            .into_any_element()
    }

    fn render_main(&self, store: &LibraryStore, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let body = match store.library() {
            None => centered("library-error", "Snowcase could not open your library", muted),
            Some(library) if library.items().is_empty() => centered(
                "empty-state",
                "Drop files or folders here, or click Add files",
                muted,
            ),
            Some(_) if self.rows.is_empty() => {
                centered("no-results", "Nothing matches this filter", muted)
            }
            Some(_) => match self.layout {
                Layout::Grid => self.render_grid(cx),
                Layout::Table => div()
                    .id("asset-table")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table))
                    .into_any_element(),
            },
        };
        v_flex()
            .size_full()
            .child(self.render_toolbar(cx))
            .child(body)
            .child(self.render_status(store, cx))
    }

    fn render_detail(&self, id: ItemId, library: &Library, cx: &Context<Self>) -> Option<AnyElement> {
        let item = library.item(id)?;
        let header = h_flex()
            .justify_between()
            .items_start()
            .gap_2()
            .child(div().font_semibold().min_w_0().child(item.title.clone()))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("open-page")
                            .ghost()
                            .compact()
                            .icon(IconName::Maximize)
                            .tooltip("Open")
                            .on_click(cx.listener(move |this, _, _, cx| this.open_page(id, cx))),
                    )
                    .child(
                        Button::new("close-detail")
                            .ghost()
                            .compact()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selected = None;
                                cx.notify();
                            })),
                    ),
            );
        Some(
            v_flex()
                .id("detail")
                .test_support()
                .size_full()
                .overflow_y_scroll()
                .gap_4()
                .p_4()
                .child(header)
                .child(self.render_overview(id, library, cx))
                .child(render_files(id, library, cx))
                .into_any_element(),
        )
    }

    fn render_overview(&self, id: ItemId, library: &Library, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let Some(item) = library.item(id) else {
            return div();
        };
        let facts = library.facts(id).cloned().unwrap_or_default();

        let status = if item.status == ItemStatus::Inbox {
            h_flex().gap_2().items_center().child(badge("Inbox", theme)).child(
                Button::new("add-to-library")
                    .primary()
                    .compact()
                    .label("Add to library")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(cx, |library| {
                            library.update_item(
                                id,
                                &ItemChanges {
                                    status: Some(ItemStatus::Confirmed),
                                    ..Default::default()
                                },
                            )
                        });
                    })),
            )
        } else {
            h_flex().child(badge("In library", theme))
        };

        let tags = h_flex().flex_wrap().gap_1().children(library.tags_of(id).into_iter().map(
            |tag| {
                let tag_id = tag.id;
                removable(format!("remove-tag-{}", tag_id.0), format!("# {}", tag.name))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(cx, |library| library.untag_item(id, tag_id));
                    }))
            },
        ));

        let folders = h_flex().flex_wrap().gap_1().children(
            library.folders_of(id).into_iter().map(|folder| {
                let folder_id = folder.id;
                removable(format!("remove-folder-{}", folder_id.0), folder.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(cx, |library| library.remove_from_folder(id, folder_id));
                    }))
            }),
        );

        v_flex()
            .gap_4()
            .child(thumbnail(facts.kinds.first().copied(), px(160.), theme))
            .child(status)
            .child(
                section("Tags")
                    .child(tags)
                    .child(Input::new(&self.tag_input).id("tag-input").small()),
            )
            .child(
                section("Folders")
                    .child(folders)
                    .child(Input::new(&self.folder_input).id("folder-input").small()),
            )
    }

    fn render_page(
        &self,
        id: ItemId,
        tab: DetailTab,
        library: &Library,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = cx.theme();
        let item = library.item(id)?;
        let selected = DETAIL_TABS
            .iter()
            .position(|(candidate, _)| *candidate == tab)
            .unwrap_or(0);
        let body = match tab {
            DetailTab::Overview => self.render_overview(id, library, cx),
            DetailTab::Files => render_files(id, library, cx),
        };
        Some(
            v_flex()
                .id("item-page")
                .test_support()
                .size_full()
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .px_4()
                        .py_2()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            Button::new("back")
                                .ghost()
                                .compact()
                                .icon(IconName::ChevronLeft)
                                .label("Library")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.page = None;
                                    cx.notify();
                                })),
                        )
                        .child(div().text_lg().font_semibold().truncate().child(item.title.clone())),
                )
                .child(
                    TabBar::new("detail-tabs")
                        .underline()
                        .px_4()
                        .selected_index(selected)
                        .children(DETAIL_TABS.iter().map(|(_, label)| Tab::new().label(*label)))
                        .on_click(cx.listener(|this, ix: &usize, _, cx| {
                            if let (Some((id, _)), Some((tab, _))) =
                                (this.page, DETAIL_TABS.get(*ix))
                            {
                                this.page = Some((id, *tab));
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .id("page-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p_6()
                        .child(div().max_w(px(720.)).child(body)),
                )
                .into_any_element(),
        )
    }
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let library = store.library();
        let content = match (self.page, library) {
            (Some((id, tab)), Some(library)) => self.render_page(id, tab, library, cx),
            _ => None,
        };
        let content = content.unwrap_or_else(|| {
            let detail = self
                .selected
                .zip(library)
                .and_then(|(id, library)| self.render_detail(id, library, cx));
            let filters = match library {
                Some(library) => self.render_filters(library, cx),
                None => div().id("filters"),
            };
            h_resizable("library-columns")
                .child(
                    resizable_panel()
                        .size(px(220.))
                        .size_range(px(160.)..px(400.))
                        .fixed()
                        .child(filters.test_support()),
                )
                .child(resizable_panel().child(self.render_main(store, cx)))
                .child(
                    resizable_panel()
                        .size(px(320.))
                        .size_range(px(260.)..px(560.))
                        .fixed()
                        .visible(detail.is_some())
                        .children(detail),
                )
                .into_any_element()
        });
        div()
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_paths(paths.paths().to_vec(), cx);
            }))
            .child(content)
    }
}

fn render_files(id: ItemId, library: &Library, cx: &App) -> Div {
    let theme = cx.theme();
    let mut list = section("Files");
    for file in library.versions(id).into_iter().flat_map(|version| version.files) {
        let missing = file
            .locations
            .first()
            .is_none_or(|location| location.state != LocationState::Present);
        list = list.child(
            h_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .child(div().flex_1().min_w_0().truncate().child(file.file.file_name.clone()))
                .when(missing, |this| this.child(badge("Missing", theme)))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format_size(file.blob.size)),
                ),
        );
    }
    list
}

fn filter_group(label: &'static str, action: Option<Button>) -> Div {
    v_flex().gap_0p5().child(
        h_flex()
            .justify_between()
            .items_center()
            .px_2()
            .pb_1()
            .child(div().text_xs().font_semibold().opacity(0.6).child(label))
            .children(action),
    )
}

fn section(label: &'static str) -> Div {
    v_flex()
        .gap_2()
        .child(div().text_xs().font_semibold().opacity(0.6).child(label))
}

fn removable(id: String, label: impl Into<SharedString>) -> Button {
    Button::new(SharedString::from(id))
        .outline()
        .compact()
        .label(format!("{}  ×", label.into()))
        .tooltip("Remove")
}

fn badge(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_1p5()
        .rounded_md()
        .text_xs()
        .bg(theme.accent)
        .text_color(theme.accent_foreground)
        .child(text.into())
}

fn centered(id: &'static str, text: &'static str, color: Hsla) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color)
        .child(text)
        .into_any_element()
}

fn thumbnail(kind: Option<FileKind>, height: Pixels, theme: &Theme) -> Div {
    let icon = match kind {
        Some(FileKind::UnityPackage | FileKind::Model) => IconName::Frame,
        Some(FileKind::Archive) => IconName::FolderClosed,
        Some(FileKind::Image) => IconName::GalleryVerticalEnd,
        Some(FileKind::Other) | None => IconName::File,
    };
    v_flex()
        .w_full()
        .h(height)
        .rounded_md()
        .bg(theme.muted)
        .items_center()
        .justify_center()
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size(px(28.)))
}

fn format_size(bytes: i64) -> String {
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
