use gpui_kit::{AppContext, Entity, TestAppContext};
use snowcase_core::StoreError;
use snowcase_core::snapshot::item_node;
use snowcase_testing::fixtures::{self, Fixture};

use super::LibraryStore;

fn store(cx: &mut TestAppContext) -> (Entity<LibraryStore>, impl Sized + use<>) {
    let Fixture { library, dir } = fixtures::library(5, 3);
    (cx.new(|cx| LibraryStore::new(Ok(library), cx)), dir)
}

fn first_item(store: &Entity<LibraryStore>, cx: &mut TestAppContext) -> snowcase_core::ItemId {
    store.read_with(cx, |store, _| store.snapshot().unwrap().items()[0].id)
}

fn tags(store: &Entity<LibraryStore>, id: snowcase_core::ItemId, cx: &mut TestAppContext) -> Vec<String> {
    store.read_with(cx, |store, _| {
        store.snapshot().unwrap().item_tags(id).iter().map(|tag| tag.name.clone()).collect()
    })
}

#[gpui_kit::test]
fn writes_run_in_the_background_and_apply_in_order(cx: &mut TestAppContext) {
    let (store, _dir) = store(cx);
    let id = first_item(&store, cx);

    store.update(cx, |store, cx| {
        store.change(cx, move |library| library.tag(&item_node(id), "first")).detach();
        store.change(cx, move |library| library.tag(&item_node(id), "second")).detach();
        store.change(cx, move |library| {
            let tag = library.item_tags(id).iter().find(|tag| tag.name == "first").unwrap().id;
            library.untag(&item_node(id), tag)
        })
        .detach();
        assert!(store.is_busy());
    });
    assert!(!tags(&store, id, cx).contains(&"second".to_owned()));

    cx.run_until_parked();

    assert_eq!(
        tags(&store, id, cx).into_iter().filter(|tag| tag == "first" || tag == "second").collect::<Vec<_>>(),
        ["second"]
    );
    store.read_with(cx, |store, _| assert!(!store.is_busy()));
}

#[gpui_kit::test]
fn a_failed_write_keeps_the_error_and_the_next_write_clears_it(cx: &mut TestAppContext) {
    let (store, _dir) = store(cx);
    let id = first_item(&store, cx);

    let failed = store.update(cx, |store, cx| {
        store.change(cx, |_| Err::<(), _>(StoreError::NewerLibrary(vec!["x".into()])))
    });
    cx.run_until_parked();
    assert_eq!(cx.foreground_executor().block_on(failed), None);
    store.read_with(cx, |store, _| assert!(store.error().is_some()));

    store.update(cx, |store, cx| store.change(cx, move |library| library.tag(&item_node(id), "ok")).detach());
    cx.run_until_parked();
    store.read_with(cx, |store, _| assert!(store.error().is_none()));
}

fn scanned(count: usize, cx: &mut TestAppContext) -> (Entity<LibraryStore>, impl Sized + use<>) {
    let (Fixture { library, dir }, assets) = fixtures::scanned(count);
    (cx.new(|cx| LibraryStore::new(Ok(library), cx)), (dir, assets))
}

fn pending(store: &Entity<LibraryStore>, cx: &mut TestAppContext) -> usize {
    let left = store.update(cx, |store, cx| store.change(cx, |library| Ok(library.pending_work().len())));
    cx.run_until_parked();
    cx.foreground_executor().block_on(left).unwrap()
}

fn progress(store: &Entity<LibraryStore>, cx: &mut TestAppContext) -> super::Progress {
    store.read_with(cx, |store, _| store.progress().clone())
}

#[gpui_kit::test]
fn background_work_runs_every_file_and_reports_progress(cx: &mut TestAppContext) {
    let (store, _dirs) = scanned(3, cx);
    assert_eq!(pending(&store, cx), 3);

    store.update(cx, |store, cx| store.start_background(cx));
    assert!(progress(&store, cx).running);
    cx.run_until_parked();

    let done = progress(&store, cx);
    assert_eq!((done.done, done.total, done.running), (3, 3, false));
    assert_eq!(done.bytes_done, done.bytes_total);
    assert!(done.failures.is_empty());
    assert_eq!(pending(&store, cx), 0);
}

#[gpui_kit::test]
fn paused_work_waits_until_resumed(cx: &mut TestAppContext) {
    let (store, _dirs) = scanned(3, cx);

    store.update(cx, |store, cx| {
        store.start_background(cx);
        store.pause_background(cx);
    });
    cx.run_until_parked();
    let paused = progress(&store, cx);
    assert_eq!((paused.done, paused.total, paused.paused, paused.running), (0, 3, true, true));

    store.update(cx, |store, cx| store.resume_background(cx));
    cx.run_until_parked();
    assert_eq!(progress(&store, cx).done, 3);
    assert_eq!(pending(&store, cx), 0);
}

#[gpui_kit::test]
fn cancelled_work_stops_and_starting_again_finishes_what_is_left(cx: &mut TestAppContext) {
    let (store, _dirs) = scanned(3, cx);

    store.update(cx, |store, cx| {
        store.start_background(cx);
        store.pause_background(cx);
    });
    cx.run_until_parked();
    store.update(cx, |store, cx| store.cancel_background(cx));
    cx.run_until_parked();
    assert!(!progress(&store, cx).running);
    assert_eq!(pending(&store, cx), 3);

    store.update(cx, |store, cx| store.start_background(cx));
    cx.run_until_parked();
    assert_eq!(pending(&store, cx), 0);
}

#[gpui_kit::test]
fn files_that_cannot_be_read_are_reported_and_the_rest_still_run(cx: &mut TestAppContext) {
    let (Fixture { library, dir }, assets) = fixtures::scanned(3);
    std::fs::remove_file(assets.path().join("Outfit01.unitypackage")).unwrap();
    let store = cx.new(|cx| LibraryStore::new(Ok(library), cx));

    store.update(cx, |store, cx| store.start_background(cx));
    cx.run_until_parked();

    let done = progress(&store, cx);
    assert_eq!(done.done, 3);
    assert_eq!(done.failures.len(), 1);
    assert!(done.failures[0].0.ends_with("Outfit01.unitypackage"));
    assert_eq!(pending(&store, cx), 1);
    drop(dir);
}

#[gpui_kit::test]
fn work_found_while_a_run_is_going_is_picked_up_when_it_ends(cx: &mut TestAppContext) {
    let (Fixture { library, dir: _dir }, assets) = fixtures::scanned(2);
    let store = cx.new(|cx| LibraryStore::new(Ok(library), cx));
    store.update(cx, |store, cx| {
        store.start_background(cx);
        store.pause_background(cx);
    });
    cx.run_until_parked();

    std::fs::write(assets.path().join("Later.unitypackage"), fixtures::unitypackage(&["Assets/Shop/Later/Later.prefab"])).unwrap();
    let root = assets.path().to_path_buf();
    store.update(cx, |store, cx| {
        store
            .change(cx, move |library| library.scan_root(&root, snowcase_core::ingest::names::CodePage::Other))
            .detach();
        store.start_background(cx);
        store.resume_background(cx);
    });
    cx.run_until_parked();

    assert_eq!(pending(&store, cx), 0);
    assert!(!progress(&store, cx).running);
}
