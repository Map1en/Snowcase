use gpui_kit::{AppContext, Entity, TestAppContext};
use snowcase_core::StoreError;
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
        store.snapshot().unwrap().tags_of(id).iter().map(|tag| tag.name.clone()).collect()
    })
}

#[gpui_kit::test]
fn writes_run_in_the_background_and_apply_in_order(cx: &mut TestAppContext) {
    let (store, _dir) = store(cx);
    let id = first_item(&store, cx);

    store.update(cx, |store, cx| {
        store.change(cx, move |library| library.tag_item(id, "first")).detach();
        store.change(cx, move |library| library.tag_item(id, "second")).detach();
        store.change(cx, move |library| {
            let tag = library.tags_of(id).iter().find(|tag| tag.name == "first").unwrap().id;
            library.untag_item(id, tag)
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

    store.update(cx, |store, cx| store.change(cx, move |library| library.tag_item(id, "ok")).detach());
    cx.run_until_parked();
    store.read_with(cx, |store, _| assert!(store.error().is_none()));
}
