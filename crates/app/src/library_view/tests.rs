use std::fs;
use std::path::PathBuf;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, ExternalPaths, FileDropEvent, Point,
    TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point, px, size,
};

use super::LibraryView;

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<LibraryView>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        let (window, view) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(800.), px(600.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| LibraryView::new(window, cx)),
        )
        .expect("open test window");
        (window, view)
    })
}

fn drop_paths(window: AnyWindowHandle, paths: Vec<PathBuf>, cx: &mut TestAppContext) {
    let position = point(px(400.), px(300.));
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(FileDropEvent::Entered {
        position,
        paths: ExternalPaths(paths.into_iter().collect()),
    });
    vcx.simulate_event(FileDropEvent::Submit { position });
}

fn row_count(window: AnyWindowHandle, cx: &mut TestAppContext) -> usize {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        (0usize..)
            .take_while(|ix| window.try_find(("asset", *ix)).is_some())
            .count()
    })
    .unwrap()
}

#[gpui_kit::test]
fn add_files_button_adds_the_picked_files(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outfit = dir.path().join("Outfit.unitypackage");
    let cover = dir.path().join("cover.png");
    fs::write(&outfit, [0x1f, 0x8b, 0x08]).unwrap();
    fs::write(&cover, b"\x89PNG\r\n\x1a\n").unwrap();
    let (window, view) = open(cx);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("empty-state").is_some());
        window.click("add-files", cx);
    })
    .unwrap();
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.multiple);
        Some(vec![outfit.clone(), cover.clone()])
    });
    cx.run_until_parked();

    assert_eq!(row_count(window, cx), 2);
    view.read_with(cx, |view, _| {
        let names: Vec<_> = view.library.assets().iter().map(|asset| asset.file.name.as_str()).collect();
        assert_eq!(names, ["Outfit.unitypackage", "cover.png"]);
        assert_eq!(view.pending_adds, 0);
    });
    cx.update_window(window, |_, window, _| assert!(window.try_find("empty-state").is_none()))
        .unwrap();
}

#[gpui_kit::test]
fn dropping_files_adds_them_and_counts_failures(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("body.fbx");
    fs::write(&model, b"Kaydara FBX Binary  \x00").unwrap();
    let (window, view) = open(cx);

    drop_paths(window, vec![model, dir.path().join("missing.zip")], cx);
    cx.run_until_parked();

    assert_eq!(row_count(window, cx), 1);
    view.read_with(cx, |view, _| {
        assert_eq!(view.library.assets().len(), 1);
        assert_eq!(view.last_failures, 1);
    });
}

#[gpui_kit::test]
fn dropping_the_same_file_twice_keeps_one_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("hair.zip");
    fs::write(&archive, b"PK\x03\x04").unwrap();
    let (window, _) = open(cx);

    drop_paths(window, vec![archive.clone()], cx);
    cx.run_until_parked();
    drop_paths(window, vec![archive], cx);
    cx.run_until_parked();

    assert_eq!(row_count(window, cx), 1);
}

#[gpui_kit::test]
fn table_layout_and_scope_filters_show_the_matching_assets(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("hair.zip");
    let cover = dir.path().join("cover.png");
    fs::write(&archive, b"PK\x03\x04").unwrap();
    fs::write(&cover, b"\x89PNG\r\n\x1a\n").unwrap();
    let (window, view) = open(cx);
    drop_paths(window, vec![archive, cover], cx);
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("layout-table", cx);
    })
    .unwrap();
    view.read_with(cx, |view, _| assert_eq!(view.layout, super::Layout::Table));
    assert_eq!(row_count(window, cx), 2);

    cx.update_window(window, |_, window, cx| window.click("scope-kind-Image", cx))
        .unwrap();
    assert_eq!(row_count(window, cx), 1);
    cx.update_window(window, |_, window, cx| window.click("scope-untagged", cx))
        .unwrap();
    assert_eq!(row_count(window, cx), 2);
}
