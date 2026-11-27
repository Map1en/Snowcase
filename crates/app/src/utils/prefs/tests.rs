use gpui_kit::TestAppContext;

use super::{get, init, set};

#[gpui_kit::test]
fn values_survive_a_restart_and_a_broken_file_starts_empty(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Snowcase").join("prefs.json");

    cx.update(|cx| {
        init(Some(path.clone()), cx);
        assert_eq!(get::<u32>("answer", cx), None);
        set("answer", &42u32, cx);
        set("name", &"list", cx);
    });
    cx.update(|cx| {
        init(Some(path.clone()), cx);
        assert_eq!(get::<u32>("answer", cx), Some(42));
        assert_eq!(get::<String>("name", cx).as_deref(), Some("list"));
        assert_eq!(get::<u32>("name", cx), None);
    });

    std::fs::write(&path, "not json").unwrap();
    cx.update(|cx| {
        init(Some(path.clone()), cx);
        assert_eq!(get::<u32>("answer", cx), None);
    });
}
