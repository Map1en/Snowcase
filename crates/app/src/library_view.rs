mod detail;
mod grid;
mod nav;
mod status_bar;
mod top_bar;
mod widgets;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SelectEvent, SelectItem, SelectState};
use gpui_kit::component::*;
use gpui_kit::*;
use snowcase_core::ingest::files::{collect_files, inspect_file};
use snowcase_core::library::Library;
use snowcase_core::snapshot::{Filter, Scope, SortKey};
use snowcase_core::{FacetKey, ItemId, StoreError};

use crate::i18n::{Locale, tr};
use crate::library_store::LibraryStore;

pub(crate) use detail::{BoothStatus, TreeNode};

pub(crate) const RESCAN_AFTER: Duration = Duration::from_secs(60);

pub(crate) const TAG_SEPARATORS: [char; 3] = [',', '，', '、'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choice {
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

pub(crate) type Choices = Entity<SelectState<Vec<Choice>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterField {
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
    booth_input: Entity<InputState>,
    booth_status: Option<BoothStatus>,
    selects: Vec<(FilterField, Choices)>,
    pending_imports: usize,
    last_failures: usize,
    open_nodes: HashSet<TreeNode>,
    _subscriptions: Vec<Subscription>,
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

impl LibraryView {
    pub fn new(store: Entity<LibraryStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let tag_input = cx.new(|cx| InputState::new(window, cx));
        let folder_input = cx.new(|cx| InputState::new(window, cx));
        let booth_input = cx.new(|cx| InputState::new(window, cx));
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
                let due = this.store.read(cx).last_scan().is_none_or(|last| now.duration_since(last) >= RESCAN_AFTER);
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
            cx.subscribe_in(&booth_input, window, |this, input, event, _, cx| {
                if let (InputEvent::PressEnter { .. }, Some(id)) = (event, this.selected) {
                    let text = input.read(cx).value().to_string();
                    this.link_booth(id, &text, cx);
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
            booth_input,
            booth_status: None,
            selects,
            pending_imports: 0,
            last_failures: 0,
            open_nodes: HashSet::new(),
            _subscriptions: subscriptions,
        };
        view.apply_locale(window, cx);
        view.refresh_choices(window, cx);
        view.refresh(cx);
        view.scan(None, cx);
        view
    }

    pub(crate) fn apply_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (input, key) in [
            (&self.search, "search.placeholder"),
            (&self.tag_input, "tag.placeholder"),
            (&self.folder_input, "folder.placeholder"),
            (&self.booth_input, "booth.placeholder"),
        ] {
            let placeholder = tr!(cx, key);
            input.update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
        }
    }

    pub(crate) fn set_filter(&mut self, field: FilterField, value: Option<String>) {
        match field {
            FilterField::Category => self.filter.category = value,
            FilterField::Target => self.filter.target = value,
            FilterField::Shop => self.filter.shop = value,
            FilterField::Tag => {
                self.filter.tag = value.and_then(|value| value.parse().ok()).map(snowcase_core::TagId);
            }
        }
    }

    pub(crate) fn refresh_choices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(crate) fn change<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Library) -> Result<T, StoreError> + Send + 'static,
    ) {
        self.store
            .update(cx, |store, cx| store.change(cx, change))
            .detach();
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn scan(&mut self, new_root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| store.scan(new_root, cx));
    }

    pub(crate) fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
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

    pub(crate) fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.filter.scope = scope;
        self.refresh(cx);
    }

    pub(crate) fn select(&mut self, id: Option<ItemId>, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected != id {
            self.tag_input.update(cx, |input, cx| input.set_value("", window, cx));
            self.open_nodes.clear();
            self.booth_status = None;
            self.booth_input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.selected = id;
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
