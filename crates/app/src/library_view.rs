use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use snowcase_core::library::{
    Filter, Hit, Library, PackageSummary, Scope, Snapshot, SortKey, collect_files, inspect_file,
    item_node, shown_name,
};
use snowcase_core::names::CodePage;
use snowcase_core::{FacetKey, FileKind, ItemId, LocationState, StoreError};

use crate::i18n::{self, Locale, tr};
use crate::library_store::LibraryStore;

const CARD_WIDTH: f32 = 172.;
const CARD_HEIGHT: f32 = 236.;
const GRID_GAP: f32 = 12.;
const GRID_PADDING: f32 = 16.;
const NAV_WIDTH: f32 = 220.;
const DETAIL_WIDTH: f32 = 460.;
const SUGGESTIONS: usize = 6;
const RESCAN_AFTER: Duration = Duration::from_secs(60);
const UNSUPPORTED_LISTED: usize = 20;
const TAG_SEPARATORS: [char; 3] = [',', '，', '、'];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    value: String,
    title: SharedString,
}

impl SelectItem for Choice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

type Choices = Entity<SelectState<Vec<Choice>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterField {
    Category,
    Target,
    Shop,
    Tag,
}

pub struct LibraryView {
    store: Entity<LibraryStore>,
    filter: Filter,
    sort: SortKey,
    nav_open: bool,
    more_filters: bool,
    rows: Vec<ItemId>,
    selected: Option<ItemId>,
    grid_width: Pixels,
    search: Entity<InputState>,
    tag_input: Entity<InputState>,
    folder_input: Entity<InputState>,
    selects: Vec<(FilterField, Choices)>,
    pending_imports: usize,
    last_failures: usize,
    scanning: bool,
    last_scan: Option<Instant>,
    unsupported: Vec<String>,
    _subscriptions: Vec<Subscription>,
}

impl LibraryView {
    pub fn new(store: Entity<LibraryStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let tag_input = cx.new(|cx| InputState::new(window, cx));
        let folder_input = cx.new(|cx| InputState::new(window, cx));
        let selects: Vec<(FilterField, Choices)> =
            [FilterField::Category, FilterField::Target, FilterField::Shop, FilterField::Tag]
                .into_iter()
                .map(|field| (field, cx.new(|cx| SelectState::new(Vec::new(), None, window, cx))))
                .collect();
        let mut subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.refresh_choices(window, cx);
                this.refresh(cx);
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                let now = cx.background_executor().now();
                let due = this.last_scan.is_none_or(|last| now.duration_since(last) >= RESCAN_AFTER);
                if window.is_window_active() && due {
                    this.scan(None, cx);
                }
            }),
            cx.observe_global_in::<Locale>(window, |this, window, cx| {
                this.apply_locale(window, cx);
                this.refresh_choices(window, cx);
                this.refresh(cx);
            }),
            cx.subscribe_in(&search, window, |this, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.filter.search = input.read(cx).value().to_string();
                    this.refresh(cx);
                }
            }),
            cx.subscribe_in(&tag_input, window, |this, input, event, window, cx| {
                let text = input.read(cx).value().to_string();
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.add_tag(text, cx);
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    InputEvent::Change if text.contains(TAG_SEPARATORS) => {
                        let mut parts: Vec<&str> = text.split(TAG_SEPARATORS).collect();
                        let rest = parts.pop().unwrap_or_default().to_owned();
                        for part in parts {
                            this.add_tag(part.to_owned(), cx);
                        }
                        input.update(cx, |input, cx| input.set_value(rest, window, cx));
                    }
                    InputEvent::Change => cx.notify(),
                    _ => {}
                }
            }),
            cx.subscribe_in(&folder_input, window, |this, input, event, window, cx| {
                let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.selected) else {
                    return;
                };
                let name = input.read(cx).value();
                this.change(cx, move |library| {
                    if let Some(folder) = library.create_folder(None, &name)? {
                        library.put_in_folder(id, folder)?;
                    }
                    Ok(())
                });
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }),
        ];
        for (field, select) in &selects {
            let field = *field;
            subscriptions.push(cx.subscribe_in(
                select,
                window,
                move |this, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    let SelectEvent::Confirm(value) = event;
                    this.set_filter(field, value.clone());
                    this.refresh(cx);
                },
            ));
        }
        let mut view = Self {
            store,
            filter: Filter::default(),
            sort: SortKey::Added,
            nav_open: true,
            more_filters: false,
            rows: Vec::new(),
            selected: None,
            grid_width: px(0.),
            search,
            tag_input,
            folder_input,
            selects,
            pending_imports: 0,
            last_failures: 0,
            scanning: false,
            last_scan: None,
            unsupported: Vec::new(),
            _subscriptions: subscriptions,
        };
        view.apply_locale(window, cx);
        view.refresh_choices(window, cx);
        view.refresh(cx);
        view.scan(None, cx);
        view
    }

    fn apply_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (input, key) in [
            (&self.search, "search.placeholder"),
            (&self.tag_input, "tag.placeholder"),
            (&self.folder_input, "folder.placeholder"),
        ] {
            let placeholder = tr!(cx, key);
            input.update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
        }
    }

    fn set_filter(&mut self, field: FilterField, value: Option<String>) {
        match field {
            FilterField::Category => self.filter.category = value,
            FilterField::Target => self.filter.target = value,
            FilterField::Shop => self.filter.shop = value,
            FilterField::Tag => {
                self.filter.tag = value.and_then(|value| value.parse().ok()).map(snowcase_core::TagId);
            }
        }
    }

    fn refresh_choices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.store.read(cx).snapshot() else {
            return;
        };
        let counted = |values: Vec<(String, usize)>| -> Vec<Choice> {
            values
                .into_iter()
                .map(|(value, count)| Choice {
                    title: format!("{value}  {count}").into(),
                    value,
                })
                .collect()
        };
        let choices: Vec<(FilterField, Vec<Choice>)> = vec![
            (FilterField::Category, counted(library.values(FacetKey::Category))),
            (FilterField::Target, counted(library.values(FacetKey::Target))),
            (FilterField::Shop, counted(library.shops())),
            (
                FilterField::Tag,
                library
                    .tag_counts()
                    .into_iter()
                    .map(|(tag, count)| Choice {
                        value: tag.id.0.to_string(),
                        title: format!("#{}  {count}", tag.name).into(),
                    })
                    .collect(),
            ),
        ];
        for (field, items) in choices {
            let Some((_, select)) = self.selects.iter().find(|(candidate, _)| *candidate == field) else {
                continue;
            };
            let kept = select.update(cx, |select, cx| {
                select.set_items(items, window, cx);
                select.selected_value().cloned()
            });
            self.set_filter(field, kept);
        }
    }

    fn change<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Library) -> Result<T, StoreError> + Send + 'static,
    ) {
        self.store
            .update(cx, |store, cx| store.change(cx, change))
            .detach();
    }

    fn add_tag(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        if name.trim().is_empty() {
            return;
        }
        self.change(cx, move |library| library.tag(&item_node(id), &name));
    }

    fn use_tag(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.add_tag(name, cx);
        self.tag_input.update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(library) = self.store.read(cx).snapshot() else {
            return;
        };
        self.rows = library
            .query(&self.filter, self.sort)
            .into_iter()
            .map(|item| item.id)
            .collect();
        self.selected = self.selected.filter(|id| library.item(*id).is_some());
        cx.notify();
    }

    fn pick_root(&mut self, cx: &mut Context<Self>) {
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

    fn scan(&mut self, new_root: Option<PathBuf>, cx: &mut Context<Self>) {
        let has_roots = self.store.read(cx).snapshot().is_some_and(|library| !library.roots().is_empty());
        if self.scanning || (new_root.is_none() && !has_roots) {
            return;
        }
        self.scanning = true;
        cx.notify();
        let code_page = code_page(i18n::system_locale());
        let scanned = self.store.update(cx, |store, cx| {
            store.change(cx, move |library| {
                if let Some(root) = &new_root {
                    library.scan_root(root, code_page)?;
                }
                library.scan_roots(code_page).map(|report| report.unsupported)
            })
        });
        cx.spawn(async move |this, cx| {
            let unsupported = scanned.await;
            this.update(cx, |this, cx| {
                this.scanning = false;
                this.last_scan = Some(cx.background_executor().now());
                if let Some(unsupported) = unsupported {
                    this.unsupported = unsupported;
                }
                this.store.update(cx, |store, cx| store.start_background(cx));
                cx.notify();
            })
            .ok();
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
                this.change(cx, move |library| library.import(files));
                this.store.update(cx, |store, cx| store.start_background(cx));
            })
            .ok();
        })
        .detach();
    }

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.filter.scope = scope;
        self.refresh(cx);
    }

    fn select(&mut self, id: Option<ItemId>, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected != id {
            self.tag_input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.selected = id;
        cx.notify();
    }

    fn add_folder(&mut self, cx: &mut Context<Self>) {
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

    fn render_top_bar(&self, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let select = |field: FilterField, id: &'static str, label: SharedString| {
            self.selects
                .iter()
                .find(|(candidate, _)| *candidate == field)
                .map(|(_, state)| {
                    div()
                        .w(px(150.))
                        .flex_shrink_0()
                        .child(Select::new(state).id(id).placeholder(label).cleanable(true).small())
                })
        };
        let main = h_flex()
            .gap_2()
            .items_center()
            .flex_wrap()
            .px_4()
            .py_2()
            .child(
                Button::new("toggle-nav")
                    .ghost()
                    .compact()
                    .icon(IconName::PanelLeft)
                    .selected(self.nav_open)
                    .tooltip(tr!(cx, "nav.toggle"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav_open = !this.nav_open;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .child(Input::new(&self.search).id("search").cleanable(true)),
            )
            .children(select(FilterField::Category, "filter-category", tr!(cx, "filter.category")))
            .children(select(FilterField::Target, "filter-target", tr!(cx, "filter.target")))
            .children(select(FilterField::Shop, "filter-shop", tr!(cx, "filter.shop")))
            .child(
                Button::new("more-filters")
                    .outline()
                    .small()
                    .label(tr!(cx, "filter.more"))
                    .selected(self.more_filters)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.more_filters = !this.more_filters;
                        cx.notify();
                    })),
            );
        v_flex()
            .border_b_1()
            .border_color(theme.border)
            .child(main)
            .when(self.more_filters, |this| {
                this.child(
                    h_flex()
                        .id("more-filter-row")
                        .test_support()
                        .gap_2()
                        .px_4()
                        .pb_2()
                        .children(select(FilterField::Tag, "filter-tag", tr!(cx, "item.tags"))),
                )
            })
    }

    fn render_nav(&self, library: &Snapshot, cx: &Context<Self>) -> impl IntoElement + use<> {
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
            .loading(self.scanning)
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
            roots = roots.child(self.nav_item(
                format!("scope-root-{}", root.id.0).into(),
                folder_name(&root.path).into(),
                0,
                count,
                Scope::Root(root.id),
                cx,
            ));
        }
        column.child(roots)
    }

    fn nav_item(
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

    fn render_results_bar(&self, total: usize, cx: &Context<Self>) -> Div {
        let sort = self.sort;
        h_flex()
            .justify_between()
            .items_center()
            .px(px(GRID_PADDING))
            .pt_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .id("result-count")
                    .child(tr!(cx, "status.count", shown = self.rows.len(), total = total)),
            )
            .child(
                ButtonGroup::new("sort")
                    .ghost()
                    .compact()
                    .children([
                        Button::new("sort-added")
                            .label(tr!(cx, "sort.newest"))
                            .selected(sort == SortKey::Added),
                        Button::new("sort-name")
                            .label(tr!(cx, "item.name"))
                            .selected(sort == SortKey::Name),
                        Button::new("sort-size")
                            .label(tr!(cx, "item.size"))
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
    }

    fn render_status(&self, store: &LibraryStore, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let progress = store.progress();
        let mut notes: Vec<SharedString> = Vec::new();
        if self.scanning {
            notes.push(tr!(cx, "status.scanning"));
        } else if self.pending_imports > 0 || store.is_busy() {
            notes.push(tr!(cx, "status.adding"));
        } else if self.last_failures > 0 {
            notes.push(tr!(cx, "status.failures", count = self.last_failures));
        }
        let unsupported = (!self.unsupported.is_empty()).then(|| {
            let listed: Vec<&str> = self.unsupported.iter().take(UNSUPPORTED_LISTED).map(String::as_str).collect();
            Button::new("unsupported")
                .ghost()
                .xsmall()
                .label(tr!(cx, "status.unsupported", count = self.unsupported.len()))
                .tooltip(listed.join("\n"))
        });
        let background = progress.running.then(|| {
            let mut label = tr!(cx, "status.background", done = progress.done, total = progress.total).to_string();
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
                    let Some(library) = store.snapshot() else {
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

    fn render_card(&self, ix: usize, library: &Snapshot, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(item) = self.rows.get(ix).and_then(|id| library.item(*id)) else {
            return div().into_any_element();
        };
        let id = item.id;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let meta: Vec<String> = [facts.attribute(FacetKey::Category).map(str::to_owned), facts.shop.clone()]
            .into_iter()
            .flatten()
            .collect();
        let meta = if meta.is_empty() { format_size(facts.size) } else { meta.join(" · ") };
        let hit = library.hit(id, &self.filter.search).map(|hit| hit_label(&hit, cx));
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
            .child(cover(library, id, px(150.), theme))
            .child(div().text_sm().font_medium().line_clamp(2).child(item.title.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(meta))
            .children(hit.map(|hit| {
                div()
                    .id(("hit", ix))
                    .test_support()
                    .px_1()
                    .rounded_sm()
                    .text_xs()
                    .bg(theme.accent)
                    .text_color(theme.accent_foreground)
                    .truncate()
                    .child(hit)
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.select(Some(id), window, cx)))
            .into_any_element()
    }

    fn render_main(&self, store: &LibraryStore, cx: &Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let total = store.snapshot().map_or(0, |library| library.items().len());
        let body = match store.snapshot() {
            None => centered("library-error", tr!(cx, "empty.no_library"), muted),
            Some(library) if library.items().is_empty() => {
                centered("empty-state", tr!(cx, "empty.library"), muted)
            }
            Some(_) if self.rows.is_empty() => {
                centered("no-results", tr!(cx, "empty.no_results"), muted)
            }
            Some(_) => self.render_grid(cx),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .when(total > 0, |this| this.child(self.render_results_bar(total, cx)))
            .child(body)
    }

    fn render_detail(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let item = library.item(id)?;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let subtitle: Vec<String> = [
            facts.shop.clone(),
            facts.attribute(FacetKey::Version).map(|version| format!("v{version}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let header = h_flex()
            .justify_between()
            .items_start()
            .gap_2()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child(item.title.clone()))
                    .when(!subtitle.is_empty(), |this| {
                        this.child(div().text_sm().text_color(theme.muted_foreground).child(subtitle.join(" · ")))
                    }),
            )
            .child(
                Button::new("close-detail")
                    .ghost()
                    .compact()
                    .icon(IconName::Close)
                    .tooltip(tr!(cx, "action.close"))
                    .on_click(cx.listener(|this, _, window, cx| this.select(None, window, cx))),
            );
        Some(
            v_flex()
                .id("detail")
                .test_support()
                .w(px(DETAIL_WIDTH))
                .flex_shrink_0()
                .h_full()
                .overflow_y_scroll()
                .gap_5()
                .p_4()
                .border_l_1()
                .border_color(theme.border)
                .child(header)
                .child(cover(library, id, px(200.), theme))
                .child(self.render_attributes(id, library, cx))
                .child(self.render_tags(id, library, cx))
                .child(self.render_folders(id, library, cx))
                .child(self.render_files(id, library, cx))
                .into_any_element(),
        )
    }

    fn render_attributes(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let facts = library.facts(id).cloned().unwrap_or_default();
        let mut chips = h_flex().flex_wrap().gap_1();
        for (ix, (key, value)) in facts.attributes.iter().cloned().enumerate() {
            let label = match key {
                FacetKey::Version => format!("v{value}"),
                _ => value.clone(),
            };
            chips = chips.child(
                removable(format!("remove-attribute-{ix}"), label, true, cx).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let value = value.clone();
                        this.change(cx, move |library| library.remove_facet(&item_node(id), key, &value));
                    },
                )),
            );
        }
        if let Some(shop) = facts.shop.clone().filter(|_| facts.attribute(FacetKey::Shop).is_none()) {
            chips = chips.child(chip(shop, theme));
        }
        let empty = facts.attributes.is_empty() && facts.shop.is_none();
        section(tr!(cx, "item.attributes"))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "item.attributes_hint")),
            )
            .child(chips)
            .when(empty, |this| {
                this.child(div().text_sm().text_color(theme.muted_foreground).child(tr!(cx, "item.no_attributes")))
            })
    }

    fn render_tags(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let own = library.tags_of(id);
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

    fn render_folders(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
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

    fn render_files(&self, id: ItemId, library: &Snapshot, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let hit_file = match library.hit(id, &self.filter.search) {
            Some(Hit::File(name)) => Some(name),
            Some(Hit::Package { file, .. }) => Some(file),
            Some(Hit::Tag { file, .. }) => file,
            _ => None,
        };
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
                let text = package_label(member, summary, cx);
                list = list.child(
                    div()
                        .id(("package", package_rows))
                        .test_support()
                        .pl_4()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(text),
                );
                package_rows += 1;
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
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let library = store.snapshot();
        let nav = library.filter(|_| self.nav_open).map(|library| self.render_nav(library, cx));
        let detail = self
            .selected
            .zip(library)
            .and_then(|(id, library)| self.render_detail(id, library, cx));
        v_flex()
            .id("library")
            .test_support()
            .size_full()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_paths(paths.paths().to_vec(), cx);
            }))
            .child(self.render_top_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .children(nav)
                    .child(self.render_main(store, cx))
                    .children(detail),
            )
            .child(self.render_status(store, cx))
    }
}

fn code_page(locale: &str) -> CodePage {
    match locale {
        "ja" => CodePage::Japanese,
        "zh-CN" => CodePage::SimplifiedChinese,
        "zh-TW" => CodePage::TraditionalChinese,
        "ko" => CodePage::Korean,
        _ => CodePage::Other,
    }
}

fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |name| name.to_string_lossy().into_owned())
}

fn package_label(member: Option<&str>, summary: &PackageSummary, cx: &App) -> String {
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
    format!("└ {}", parts.join(" · "))
}

fn hit_label(hit: &Hit, cx: &App) -> SharedString {
    match hit {
        Hit::File(name) | Hit::Attribute(name) => tr!(cx, "hit.match", name = name),
        Hit::Package { package, path, .. } => {
            tr!(cx, "hit.package", name = path.rsplit('/').next().unwrap_or(path), package = package)
        }
        Hit::Tag { name, file: None } => tr!(cx, "hit.tag", name = name),
        Hit::Tag { name, file: Some(file) } => tr!(cx, "hit.tag_in", name = name, file = file),
    }
}

fn filter_group(label: SharedString, actions: impl IntoIterator<Item = Button>) -> Div {
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

fn section(label: SharedString) -> Div {
    v_flex()
        .gap_2()
        .child(div().text_xs().font_semibold().opacity(0.6).child(label))
}

fn removable(id: String, label: impl Into<SharedString>, light: bool, cx: &App) -> Button {
    let button = Button::new(SharedString::from(id))
        .compact()
        .small()
        .label(format!("{}  ×", label.into()))
        .tooltip(tr!(cx, "action.remove"));
    if light { button.ghost() } else { button.outline() }
}

fn chip(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_sm()
        .bg(theme.muted)
        .text_color(theme.muted_foreground)
        .child(text.into())
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

fn hint(text: SharedString, theme: &Theme) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .bg(theme.muted)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn centered(id: &'static str, text: SharedString, color: Hsla) -> AnyElement {
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

fn cover(library: &Snapshot, id: ItemId, height: Pixels, theme: &Theme) -> Div {
    let file = library.main_file(id);
    let file_name = file.map(shown_name);
    let kind = file
        .and_then(|file| library.files(id).into_iter().find(|view| view.file.blob_id == file.blob_id))
        .map(|view| FileKind::from(view.blob.kind));
    let icon = match kind {
        Some(FileKind::UnityPackage | FileKind::Model) => IconName::Frame,
        Some(FileKind::Archive) => IconName::FolderClosed,
        Some(FileKind::Image) => IconName::GalleryVerticalEnd,
        Some(FileKind::Other) | None => IconName::File,
    };
    let extension = file_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_uppercase());
    v_flex()
        .w_full()
        .h(height)
        .flex_shrink_0()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.muted)
        .items_center()
        .justify_center()
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size(px(28.)))
        .children(extension.map(|extension| div().text_xs().font_semibold().child(extension)))
        .children(file_name.map(|name| {
            div()
                .w_full()
                .text_xs()
                .text_center()
                .line_clamp(2)
                .child(name.to_owned())
        }))
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
